import { useState } from "react";
import { clearSenseCache, testLlm, testSenseSelector, toIpcError, updateSettings } from "../../lib/ipc";
import type {
  AiDefinition,
  IpcError,
  LlmSettings,
  SenseResolution,
  SenseSettings,
  SenseStatus,
  Settings,
} from "../../lib/types";
import { Button } from "../../components/Button";
import { Field } from "../../components/Field";

const STATUS_LABELS: Record<SenseStatus, string> = {
  disabled: "未启用",
  no_context: "缺少原句",
  selected: "已推荐",
  ambiguous: "建议确认",
  low_confidence: "低置信度",
  unavailable: "选择器不可用",
  failed: "失败",
};

interface FieldErrors {
  selectorEndpoint?: string;
  selectorTimeout?: string;
  confidence?: string;
  llmEndpoint?: string;
  llmTimeout?: string;
  maxTokens?: string;
  apiKeyEnv?: string;
}

/** Accepts only loopback hosts; remote selectors need an explicit opt-in. */
function isLoopbackEndpoint(endpoint: string): boolean {
  try {
    const host = new URL(endpoint).hostname.toLowerCase();
    return host === "127.0.0.1" || host === "localhost" || host === "::1" || host === "[::1]";
  } catch {
    return false;
  }
}

function hasHttpScheme(value: string): boolean {
  try {
    const parsed = new URL(value);
    return parsed.protocol === "http:" || parsed.protocol === "https:";
  } catch {
    return false;
  }
}

function confidenceLabel(confidence: number | null): string {
  return confidence === null ? "未提供" : `${confidence.toFixed(2)}（模型自评）`;
}

interface SenseSectionProps {
  settings: Settings;
  onSaved: (next: Settings) => void;
}

/**
 * 语境辅助设置（APP_FLOW §7.5, PLAN_M4 T4.9）：选择器、阈值、LLM 兜底、
 * 回环限制、测试按钮与语境缓存清理。置信度只作为模型自评展示，不是校准概率。
 */
export function SenseSection({ settings, onSaved }: SenseSectionProps) {
  const [sense, setSense] = useState<SenseSettings>({ ...settings.sense });
  const [llm, setLlm] = useState<LlmSettings>({ ...settings.llm });
  const [errors, setErrors] = useState<FieldErrors>({});
  const [saveError, setSaveError] = useState<IpcError | null>(null);
  const [saving, setSaving] = useState(false);
  const [saved, setSaved] = useState(false);

  const [testWord, setTestWord] = useState("");
  const [testSentence, setTestSentence] = useState("");
  const [testFormError, setTestFormError] = useState<string | null>(null);
  const [testingSelector, setTestingSelector] = useState(false);
  const [testingLlm, setTestingLlm] = useState(false);
  const [selectorResult, setSelectorResult] = useState<SenseResolution | null>(null);
  const [selectorError, setSelectorError] = useState<IpcError | null>(null);
  const [llmResult, setLlmResult] = useState<AiDefinition | null>(null);
  const [llmError, setLlmError] = useState<IpcError | null>(null);

  const [confirmClear, setConfirmClear] = useState(false);
  const [clearing, setClearing] = useState(false);
  const [clearError, setClearError] = useState<IpcError | null>(null);
  const [clearNotice, setClearNotice] = useState<string | null>(null);

  const updateSense = <K extends keyof SenseSettings>(key: K, value: SenseSettings[K]) => {
    setSense((current) => ({ ...current, [key]: value }));
    setSaved(false);
    setSaveError(null);
  };

  const updateLlm = <K extends keyof LlmSettings>(key: K, value: LlmSettings[K]) => {
    setLlm((current) => ({ ...current, [key]: value }));
    setSaved(false);
    setSaveError(null);
  };

  const validate = (): FieldErrors => {
    const next: FieldErrors = {};

    const endpoint = sense.selector_endpoint.trim();
    if (endpoint.length === 0) {
      next.selectorEndpoint = "请输入选择器地址。";
    } else if (!hasHttpScheme(endpoint)) {
      next.selectorEndpoint = "选择器地址只支持 http/https。";
    } else if (!isLoopbackEndpoint(endpoint) && !sense.selector_allow_remote) {
      next.selectorEndpoint = "选择器地址不是回环地址，需勾选“允许远程选择器”。";
    }

    if (
      !Number.isInteger(sense.selector_timeout_ms) ||
      sense.selector_timeout_ms < 200 ||
      sense.selector_timeout_ms > 3000
    ) {
      next.selectorTimeout = "选择器超时需为 200–3000 的整数（毫秒）。";
    }

    if (
      !Number.isFinite(sense.low_confidence) ||
      !Number.isFinite(sense.high_confidence) ||
      sense.low_confidence <= 0 ||
      sense.low_confidence >= sense.high_confidence ||
      sense.high_confidence > 1
    ) {
      next.confidence = "需满足 0 < 低阈值 < 高阈值 ≤ 1。";
    }

    const llmEndpoint = llm.endpoint.trim();
    if (llmEndpoint.length === 0) {
      next.llmEndpoint = "请输入 LLM 端点。";
    } else if (!hasHttpScheme(llmEndpoint)) {
      next.llmEndpoint = "LLM 端点只支持 http/https。";
    } else if (!isLoopbackEndpoint(llmEndpoint) && !llm.allow_remote) {
      next.llmEndpoint = "LLM 端点不是回环地址，需勾选“允许远程 LLM”。";
    }

    if (
      !Number.isInteger(llm.timeout_ms) ||
      llm.timeout_ms < 1000 ||
      llm.timeout_ms > 60000
    ) {
      next.llmTimeout = "LLM 超时需为 1000–60000 的整数（毫秒）。";
    }

    if (!Number.isInteger(llm.max_tokens) || llm.max_tokens < 1 || llm.max_tokens > 4096) {
      next.maxTokens = "最大 token 需为 1–4096 的整数。";
    }

    const envName = llm.api_key_env.trim();
    if (envName.length > 0 && !/^[A-Za-z_][A-Za-z0-9_]*$/.test(envName)) {
      next.apiKeyEnv = "环境变量名只能包含字母、数字与下划线，且不能以数字开头。";
    }

    return next;
  };

  const save = async () => {
    if (saving) return;
    const nextErrors = validate();
    setErrors(nextErrors);
    if (Object.keys(nextErrors).length > 0) return;

    const cleanSense: SenseSettings = {
      ...sense,
      selector_endpoint: sense.selector_endpoint.trim(),
    };
    const cleanLlm: LlmSettings = {
      ...llm,
      endpoint: llm.endpoint.trim(),
      model: llm.model.trim(),
      api_key_env: llm.api_key_env.trim(),
    };

    setSaving(true);
    setSaveError(null);
    try {
      const updated = await updateSettings({ ...settings, sense: cleanSense, llm: cleanLlm });
      onSaved(updated);
      setSense({ ...updated.sense });
      setLlm({ ...updated.llm });
      setSaved(true);
    } catch (cause: unknown) {
      setSaveError(toIpcError(cause));
    } finally {
      setSaving(false);
    }
  };

  const validateTestInputs = (): boolean => {
    if (testWord.trim().length === 0) {
      setTestFormError("请输入测试词。");
      return false;
    }
    if (testSentence.trim().length === 0) {
      setTestFormError("请输入测试原句；没有原句时语境辅助不会触发。");
      return false;
    }
    setTestFormError(null);
    return true;
  };

  const runSelectorTest = async () => {
    if (testingSelector || !validateTestInputs()) return;
    setTestingSelector(true);
    setSelectorResult(null);
    setSelectorError(null);
    try {
      setSelectorResult(await testSenseSelector(testWord.trim(), testSentence.trim()));
    } catch (cause: unknown) {
      setSelectorError(toIpcError(cause));
    } finally {
      setTestingSelector(false);
    }
  };

  const runLlmTest = async () => {
    if (testingLlm || !validateTestInputs()) return;
    setTestingLlm(true);
    setLlmResult(null);
    setLlmError(null);
    try {
      setLlmResult(await testLlm(testWord.trim(), testSentence.trim(), []));
    } catch (cause: unknown) {
      setLlmError(toIpcError(cause));
    } finally {
      setTestingLlm(false);
    }
  };

  const runClear = async () => {
    if (clearing) return;
    setClearing(true);
    setClearError(null);
    setClearNotice(null);
    try {
      const result = await clearSenseCache();
      setClearNotice(`已清除 ${result.removed} 条语境缓存。`);
      setConfirmClear(false);
    } catch (cause: unknown) {
      setClearError(toIpcError(cause));
    } finally {
      setClearing(false);
    }
  };

  const dirty =
    JSON.stringify(sense) !== JSON.stringify(settings.sense) ||
    JSON.stringify(llm) !== JSON.stringify(settings.llm);

  return (
    <section aria-labelledby="sense-title" className="flex min-h-full max-w-[560px] flex-col gap-8">
      <header>
        <h1 id="sense-title" className="text-[26px] font-medium tracking-tight">
          语境辅助
        </h1>
        <p className="mt-1 text-sm text-ink-muted">
          用原句在候选义项中做推荐；低置信度时可选用 LLM 生成词典外释义。失败只影响辅助区。
        </p>
      </header>

      <form
        noValidate
        onSubmit={(event) => {
          event.preventDefault();
          void save();
        }}
        className="flex flex-col gap-8"
      >
        <fieldset className="flex flex-col gap-4 rounded-3xl border border-hairline px-6 py-5">
          <legend className="px-1 text-sm font-medium">选择器</legend>

          <label className="flex items-start gap-3 text-sm">
            <input
              type="checkbox"
              checked={sense.enabled}
              onChange={(event) => updateSense("enabled", event.target.checked)}
              className="mt-0.5 size-4 accent-accent"
            />
            <span>
              <span className="block font-medium">启用语境辅助选择器</span>
              <span className="mt-0.5 block text-xs text-ink-muted">
                关闭时只展示全部词典义项，不调用选择器。
              </span>
            </span>
          </label>

          <Field
            label="选择器地址"
            id="sense-endpoint"
            hint="默认 http://127.0.0.1:8787/v1/select；默认只允许回环地址。"
            error={errors.selectorEndpoint ?? null}
          >
            <input
              value={sense.selector_endpoint}
              onChange={(event) => {
                updateSense("selector_endpoint", event.target.value);
                setErrors((current) => ({ ...current, selectorEndpoint: undefined }));
              }}
              className="h-10 rounded-lg border border-hairline bg-canvas px-3 font-mono text-xs outline-none focus:ring-2 focus:ring-accent focus:ring-offset-1"
              placeholder="http://127.0.0.1:8787/v1/select"
            />
          </Field>

          <Field
            label="选择器超时（毫秒）"
            id="sense-timeout"
            hint="范围 200–3000；超时只影响推荐区。"
            error={errors.selectorTimeout ?? null}
          >
            <input
              inputMode="numeric"
              value={sense.selector_timeout_ms}
              onChange={(event) => {
                updateSense(
                  "selector_timeout_ms",
                  Number.parseInt(event.target.value, 10) || 0,
                );
                setErrors((current) => ({ ...current, selectorTimeout: undefined }));
              }}
              className="h-10 rounded-lg border border-hairline bg-canvas px-3 text-sm outline-none focus:ring-2 focus:ring-accent focus:ring-offset-1"
            />
          </Field>

          <div className="flex flex-col gap-2">
            <label className="flex items-start gap-3 text-sm">
              <input
                type="checkbox"
                checked={sense.selector_allow_remote}
                onChange={(event) => updateSense("selector_allow_remote", event.target.checked)}
                className="mt-0.5 size-4 accent-accent"
              />
              <span>
                <span className="block font-medium">允许远程选择器</span>
                <span className="mt-0.5 block text-xs text-ink-muted">
                  默认只允许 127.0.0.1、localhost 与 ::1。
                </span>
              </span>
            </label>
            {sense.selector_allow_remote ? (
              <div
                role="note"
                className="rounded-xl border border-warning/40 bg-canvas-soft px-4 py-3 text-xs"
              >
                <p className="font-medium text-warning">原句会发送到该服务</p>
                <p className="mt-1 text-ink-muted">
                  勾选后允许非回环选择器；确认该服务可信后再保存。原句可能包含页面上下文。
                </p>
              </div>
            ) : null}
          </div>

          <div className="grid grid-cols-2 gap-4">
            <Field
              label="低置信度阈值"
              id="sense-low"
              hint="默认 0.55。"
              error={errors.confidence ?? null}
            >
              <input
                inputMode="decimal"
                value={sense.low_confidence}
                onChange={(event) => {
                  updateSense("low_confidence", Number.parseFloat(event.target.value) || 0);
                  setErrors((current) => ({ ...current, confidence: undefined }));
                }}
                className="h-10 rounded-lg border border-hairline bg-canvas px-3 text-sm outline-none focus:ring-2 focus:ring-accent focus:ring-offset-1"
              />
            </Field>
            <Field label="高置信度阈值" id="sense-high" hint="默认 0.72。">
              <input
                inputMode="decimal"
                value={sense.high_confidence}
                onChange={(event) => {
                  updateSense("high_confidence", Number.parseFloat(event.target.value) || 0);
                  setErrors((current) => ({ ...current, confidence: undefined }));
                }}
                className="h-10 rounded-lg border border-hairline bg-canvas px-3 text-sm outline-none focus:ring-2 focus:ring-accent focus:ring-offset-1"
              />
            </Field>
          </div>
          <p className="text-xs text-ink-muted">
            阈值需满足 0 &lt; 低阈值 &lt; 高阈值 ≤ 1。置信度是模型自评，非绝对概率，不应当作准确率。
          </p>
        </fieldset>

        <fieldset className="flex flex-col gap-4 rounded-3xl border border-hairline px-6 py-5">
          <legend className="px-1 text-sm font-medium">LLM 兜底</legend>

          <label className="flex items-start gap-3 text-sm">
            <input
              type="checkbox"
              checked={llm.enabled}
              onChange={(event) => updateLlm("enabled", event.target.checked)}
              className="mt-0.5 size-4 accent-accent"
            />
            <span>
              <span className="block font-medium">低置信度时生成 AI 释义</span>
              <span className="mt-0.5 block text-xs text-ink-muted">
                仅在有原句且选择器低置信度时调用；AI 释义需用户确认后才写入卡片。
              </span>
            </span>
          </label>

          <Field
            label="LLM 端点"
            id="llm-endpoint"
            hint="OpenAI-compatible；默认只允许回环地址。"
            error={errors.llmEndpoint ?? null}
          >
            <input
              value={llm.endpoint}
              onChange={(event) => {
                updateLlm("endpoint", event.target.value);
                setErrors((current) => ({ ...current, llmEndpoint: undefined }));
              }}
              className="h-10 rounded-lg border border-hairline bg-canvas px-3 font-mono text-xs outline-none focus:ring-2 focus:ring-accent focus:ring-offset-1"
              placeholder="http://127.0.0.1:11434/v1/chat/completions"
            />
          </Field>

          <div className="grid grid-cols-2 gap-4">
            <Field label="模型名" id="llm-model" hint="例如本地模型名称；启用 LLM 时填写。">
              <input
                value={llm.model}
                onChange={(event) => updateLlm("model", event.target.value)}
                className="h-10 rounded-lg border border-hairline bg-canvas px-3 text-sm outline-none focus:ring-2 focus:ring-accent focus:ring-offset-1"
                placeholder="qwen2.5:7b"
              />
            </Field>
            <Field
              label="API key 环境变量名"
              id="llm-api-key-env"
              hint="只读取环境变量，不写入配置、不写日志。"
              error={errors.apiKeyEnv ?? null}
            >
              <input
                value={llm.api_key_env}
                onChange={(event) => {
                  updateLlm("api_key_env", event.target.value);
                  setErrors((current) => ({ ...current, apiKeyEnv: undefined }));
                }}
                className="h-10 rounded-lg border border-hairline bg-canvas px-3 font-mono text-xs outline-none focus:ring-2 focus:ring-accent focus:ring-offset-1"
                placeholder="LEXIGRAB_LLM_API_KEY"
              />
            </Field>
          </div>

          <div className="grid grid-cols-2 gap-4">
            <Field
              label="LLM 超时（毫秒）"
              id="llm-timeout"
              hint="范围 1000–60000；默认 15000。"
              error={errors.llmTimeout ?? null}
            >
              <input
                inputMode="numeric"
                value={llm.timeout_ms}
                onChange={(event) => {
                  updateLlm("timeout_ms", Number.parseInt(event.target.value, 10) || 0);
                  setErrors((current) => ({ ...current, llmTimeout: undefined }));
                }}
                className="h-10 rounded-lg border border-hairline bg-canvas px-3 text-sm outline-none focus:ring-2 focus:ring-accent focus:ring-offset-1"
              />
            </Field>
            <Field
              label="最大 token"
              id="llm-max-tokens"
              hint="范围 1–4096；默认 320。"
              error={errors.maxTokens ?? null}
            >
              <input
                inputMode="numeric"
                value={llm.max_tokens}
                onChange={(event) => {
                  updateLlm("max_tokens", Number.parseInt(event.target.value, 10) || 0);
                  setErrors((current) => ({ ...current, maxTokens: undefined }));
                }}
                className="h-10 rounded-lg border border-hairline bg-canvas px-3 text-sm outline-none focus:ring-2 focus:ring-accent focus:ring-offset-1"
              />
            </Field>
          </div>

          <div className="flex flex-col gap-2">
            <label className="flex items-start gap-3 text-sm">
              <input
                type="checkbox"
                checked={llm.allow_remote}
                onChange={(event) => updateLlm("allow_remote", event.target.checked)}
                className="mt-0.5 size-4 accent-accent"
              />
              <span>
                <span className="block font-medium">允许远程 LLM</span>
                <span className="mt-0.5 block text-xs text-ink-muted">
                  默认只允许回环端点；远程端点需要显式允许。
                </span>
              </span>
            </label>
            {llm.allow_remote ? (
              <div
                role="note"
                className="rounded-xl border border-warning/40 bg-canvas-soft px-4 py-3 text-xs"
              >
                <p className="font-medium text-warning">原句会发送到该服务</p>
                <p className="mt-1 text-ink-muted">
                  勾选后允许非回环 LLM；原句与候选义项会发送到该端点，确认服务可信后再保存。
                </p>
              </div>
            ) : null}
          </div>
        </fieldset>

        {saveError ? (
          <div role="alert" className="rounded-2xl border border-danger/40 px-4 py-3 text-sm">
            <p className="font-medium text-danger">保存失败</p>
            <p className="mt-1 text-ink-muted">{saveError.message}</p>
            <p className="mt-1 font-mono text-[11px] tracking-[0.6px] text-ink-muted">
              {saveError.code}
            </p>
          </div>
        ) : null}

        <footer className="sticky bottom-0 flex items-center justify-between gap-4 border-t border-hairline bg-canvas py-4">
          <p role="status" aria-live="polite" className="text-xs text-ink-muted">
            {saved ? "已保存" : dirty ? "有未保存的更改" : ""}
          </p>
          <Button type="submit" disabled={saving}>
            {saving ? "保存中…" : "保存"}
          </Button>
        </footer>
      </form>

      <section aria-labelledby="sense-test-title" className="rounded-3xl border border-hairline px-6 py-5">
        <h2 id="sense-test-title" className="text-[16px] font-medium">
          测试语境辅助
        </h2>
        <p className="mt-1 text-xs text-ink-muted">
          直接调用已保存的配置；不写缓存、不写卡片。修改设置后请先保存再测试。
        </p>

        <div className="mt-4 grid grid-cols-2 gap-4">
          <Field label="测试词" id="sense-test-word">
            <input
              value={testWord}
              onChange={(event) => {
                setTestWord(event.target.value);
                setTestFormError(null);
              }}
              className="h-10 rounded-lg border border-hairline bg-canvas px-3 text-sm outline-none focus:ring-2 focus:ring-accent focus:ring-offset-1"
              placeholder="例如：bank"
            />
          </Field>
          <Field label="测试原句" id="sense-test-sentence">
            <input
              value={testSentence}
              onChange={(event) => {
                setTestSentence(event.target.value);
                setTestFormError(null);
              }}
              className="h-10 rounded-lg border border-hairline bg-canvas px-3 text-sm outline-none focus:ring-2 focus:ring-accent focus:ring-offset-1"
              placeholder="例如：He sat on the bank of the river."
            />
          </Field>
        </div>
        {testFormError ? (
          <p role="alert" className="mt-2 text-xs text-danger">
            {testFormError}
          </p>
        ) : null}

        <div className="mt-4 flex flex-wrap gap-2">
          <Button
            variant="secondary"
            size="sm"
            onClick={() => void runSelectorTest()}
            disabled={testingSelector}
          >
            {testingSelector ? "测试中…" : "测试选择器"}
          </Button>
          <Button
            variant="secondary"
            size="sm"
            onClick={() => void runLlmTest()}
            disabled={testingLlm}
          >
            {testingLlm ? "测试中…" : "测试 LLM"}
          </Button>
        </div>

        <div className="mt-4 flex flex-col gap-3">
          {selectorError ? (
            <div role="alert" className="rounded-2xl border border-danger/40 px-4 py-3 text-sm">
              <p className="font-medium text-danger">选择器测试失败</p>
              <p className="mt-1 text-ink-muted">{selectorError.message}</p>
              <p className="mt-1 font-mono text-[11px] tracking-[0.6px] text-ink-muted">
                {selectorError.code}
              </p>
            </div>
          ) : null}

          {selectorResult ? (
            <div className="rounded-2xl border border-hairline px-4 py-3 text-sm">
              <div className="flex flex-wrap items-center justify-between gap-2">
                <p className="font-medium">选择器结果</p>
                <span className="text-xs text-ink-muted">
                  状态：{STATUS_LABELS[selectorResult.status]}
                </span>
              </div>
              <dl className="mt-2 grid grid-cols-2 gap-x-6 gap-y-2 text-xs">
                <div className="flex items-center justify-between gap-3">
                  <dt className="text-ink-muted">推荐义项</dt>
                  <dd className="font-mono">{selectorResult.selected_sense_id ?? "未选中"}</dd>
                </div>
                <div className="flex items-center justify-between gap-3">
                  <dt className="text-ink-muted">置信度</dt>
                  <dd className="font-mono">{confidenceLabel(selectorResult.confidence)}</dd>
                </div>
                <div className="flex items-center justify-between gap-3">
                  <dt className="text-ink-muted">模型</dt>
                  <dd className="font-mono">{selectorResult.selector_model ?? "未提供"}</dd>
                </div>
                <div className="flex items-center justify-between gap-3">
                  <dt className="text-ink-muted">候选义项</dt>
                  <dd className="font-mono">{selectorResult.candidates.length} 个</dd>
                </div>
              </dl>
              {selectorResult.error ? (
                <p role="alert" className="mt-2 text-xs text-danger">
                  {selectorResult.error}
                </p>
              ) : null}
            </div>
          ) : null}

          {llmError ? (
            <div role="alert" className="rounded-2xl border border-danger/40 px-4 py-3 text-sm">
              <p className="font-medium text-danger">LLM 测试失败</p>
              <p className="mt-1 text-ink-muted">{llmError.message}</p>
              <p className="mt-1 font-mono text-[11px] tracking-[0.6px] text-ink-muted">
                {llmError.code}
              </p>
            </div>
          ) : null}

          {llmResult ? (
            <div className="rounded-2xl border border-dashed border-hairline bg-canvas-soft px-4 py-3 text-sm">
              <p className="font-mono text-[11px] tracking-[0.6px] text-ink-muted">
                AI 生成 · {llmResult.model} · 可能不准确
              </p>
              <p className="mt-2 leading-6">{llmResult.definition_text}</p>
              <p className="mt-2 text-xs text-ink-muted">
                词性：{llmResult.part_of_speech ?? "未提供"} · 置信度：
                {confidenceLabel(llmResult.confidence)}
              </p>
            </div>
          ) : null}

          {!selectorResult && !selectorError && !llmResult && !llmError ? (
            <p className="text-xs text-ink-muted">
              输入词与原句后点击测试；选择器可用性、置信度与 AI 释义会显示在这里。
            </p>
          ) : null}
        </div>
      </section>

      <section aria-labelledby="sense-cache-title" className="rounded-3xl border border-hairline px-6 py-5">
        <h2 id="sense-cache-title" className="text-[16px] font-medium">
          语境缓存
        </h2>
        <p className="mt-1 text-xs text-ink-muted">
          语境缓存与词典缓存分开；清理后下次查询会重新请求选择器或 LLM。不删除已保存的卡片。
        </p>

        {clearNotice ? (
          <p role="status" className="mt-3 rounded-xl bg-canvas-soft px-3 py-2 text-xs text-ink-muted">
            {clearNotice}
          </p>
        ) : null}
        {clearError ? (
          <p role="alert" className="mt-3 text-xs text-danger">
            清理失败：{clearError.message}
            <span className="ml-2 font-mono text-[11px] tracking-[0.6px]">{clearError.code}</span>
          </p>
        ) : null}

        <div className="mt-3">
          {confirmClear ? (
            <div className="flex flex-wrap items-center gap-2">
              <Button size="sm" onClick={() => void runClear()} disabled={clearing}>
                {clearing ? "清理中…" : "确认清除语境缓存"}
              </Button>
              <Button
                variant="secondary"
                size="sm"
                onClick={() => setConfirmClear(false)}
                disabled={clearing}
              >
                取消
              </Button>
            </div>
          ) : (
            <Button
              variant="secondary"
              size="sm"
              onClick={() => {
                setClearError(null);
                setClearNotice(null);
                setConfirmClear(true);
              }}
            >
              清除语境缓存
            </Button>
          )}
        </div>
      </section>
    </section>
  );
}
