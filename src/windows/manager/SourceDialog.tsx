// SPDX-License-Identifier: AGPL-3.0-only
import { useEffect, useRef, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import {
  addSource,
  testSourceQuery,
  toIpcError,
  updateSource,
} from "../../lib/ipc";
import type {
  IpcError,
  RemoteSourceConfig,
  SourceConfig,
  TestResult,
} from "../../lib/types";
import { Field } from "../../components/Field";
import { Modal } from "./Modal";
import { TestResultView } from "./TestResultView";
import { basename, normalisePath, stripMdxExtension } from "./paths";

type SourceType = SourceConfig["type"];
type ParserMode = "jsonpath" | "script";

const TYPE_OPTIONS: { value: SourceType; label: string; description: string }[] = [
  {
    value: "local_file",
    label: "本地词典",
    description: "读取本机 .mdx 文件，可选同名 .mdd。",
  },
  {
    value: "remote_http",
    label: "远程 API",
    description: "按 URL 模板请求 HTTP 接口并解析响应。",
  },
];

const DEFAULT_REMOTE_TIMEOUT_MS = "3000";
const DEFAULT_JSONPATH = "$.entries[*]";

interface SourceDialogProps {
  mode: "add" | "edit";
  /** Existing source when editing; null when adding. */
  source: SourceConfig | null;
  /** Used for duplicate-path validation and priority defaults. */
  existingSources: SourceConfig[];
  onClose: () => void;
  /** Called after a successful add/update; the caller reloads the list. */
  onSaved: () => void;
}

interface FieldErrors {
  name?: string;
  paths?: string;
  priority?: string;
  urlTemplate?: string;
  auth?: string;
  timeout?: string;
  jsonpath?: string;
  scriptPath?: string;
  scriptTimeout?: string;
}

function defaultPriority(sources: SourceConfig[]): number {
  const max = sources.reduce((highest, item) => Math.max(highest, item.priority), 0);
  return max + 10;
}

/** Strict integer parse so `12abc` is rejected instead of silently becoming 12. */
function parseInteger(raw: string): number | null {
  const trimmed = raw.trim();
  if (!/^\d+$/.test(trimmed)) return null;
  const value = Number.parseInt(trimmed, 10);
  return Number.isSafeInteger(value) ? value : null;
}

function hasHttpScheme(value: string): boolean {
  try {
    const parsed = new URL(value);
    return parsed.protocol === "http:" || parsed.protocol === "https:";
  } catch {
    return false;
  }
}

/**
 * Add/edit dialog for data sources (APP_FLOW §5.2, PLAN_M4 T4.6).
 * Local sources read .mdx files; remote sources send the URL template and parse
 * the response with JSONPath or a sandboxed script. The backend generates ids.
 */
export function SourceDialog({ mode, source, existingSources, onClose, onSaved }: SourceDialogProps) {
  const initialRemote = source?.type === "remote_http" ? source : null;
  const initialParser = initialRemote?.response_parser ?? null;

  const [kind, setKind] = useState<SourceType>(source?.type ?? "local_file");
  const [name, setName] = useState(source?.name ?? "");
  const [enabled, setEnabled] = useState(source?.enabled ?? true);
  const [priority, setPriority] = useState(
    String(source?.priority ?? defaultPriority(existingSources)),
  );

  // Local source state.
  const [paths, setPaths] = useState<string[]>(
    source?.type === "local_file" ? source.paths : [],
  );
  const [nameAuto, setNameAuto] = useState(mode === "add");

  // Remote source state.
  const [urlTemplate, setUrlTemplate] = useState(initialRemote?.url_template ?? "");
  const [authHeader, setAuthHeader] = useState(initialRemote?.auth_header ?? "");
  const [authValue, setAuthValue] = useState(initialRemote?.auth_value ?? "");
  const [timeoutMs, setTimeoutMs] = useState(
    initialRemote ? String(initialRemote.timeout_ms) : DEFAULT_REMOTE_TIMEOUT_MS,
  );
  const [parserMode, setParserMode] = useState<ParserMode>(
    initialParser?.mode === "script" ? "script" : "jsonpath",
  );
  const [expression, setExpression] = useState(
    initialParser?.mode === "jsonpath" ? initialParser.expression : DEFAULT_JSONPATH,
  );
  const [scriptPath, setScriptPath] = useState(
    initialParser?.mode === "script" ? initialParser.path : "",
  );
  const [scriptTimeout, setScriptTimeout] = useState(
    initialParser?.mode === "script" && initialParser.timeout_ms !== null
      ? String(initialParser.timeout_ms)
      : "",
  );

  const [fieldErrors, setFieldErrors] = useState<FieldErrors>({});
  const [pickerError, setPickerError] = useState<string | null>(null);
  const [saveError, setSaveError] = useState<IpcError | null>(null);
  const [saving, setSaving] = useState(false);

  // Draft test panel: uses the current form values, never writes config or cache.
  const [testQuery, setTestQuery] = useState("");
  const [testQueryError, setTestQueryError] = useState<string | null>(null);
  const [testing, setTesting] = useState(false);
  const [testResult, setTestResult] = useState<TestResult | null>(null);
  const [testError, setTestError] = useState<IpcError | null>(null);

  const nameRef = useRef<HTMLInputElement>(null);
  const fileButtonRef = useRef<HTMLButtonElement>(null);
  const priorityRef = useRef<HTMLInputElement>(null);
  const urlRef = useRef<HTMLInputElement>(null);
  const authHeaderRef = useRef<HTMLInputElement>(null);
  const timeoutRef = useRef<HTMLInputElement>(null);
  const expressionRef = useRef<HTMLInputElement>(null);
  const scriptPathRef = useRef<HTMLInputElement>(null);
  const scriptTimeoutRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    nameRef.current?.focus();
  }, [mode]);

  const clearTest = () => {
    setTestResult(null);
    setTestError(null);
    setTestQueryError(null);
  };

  /** Switching type clears that type's fields (task: edit conversion refills). */
  const switchKind = (next: SourceType) => {
    if (next === kind) return;
    setKind(next);
    setFieldErrors({});
    setSaveError(null);
    clearTest();
    if (next === "remote_http") {
      setUrlTemplate("");
      setAuthHeader("");
      setAuthValue("");
      setTimeoutMs(DEFAULT_REMOTE_TIMEOUT_MS);
      setParserMode("jsonpath");
      setExpression(DEFAULT_JSONPATH);
      setScriptPath("");
      setScriptTimeout("");
    } else {
      setPaths([]);
      setNameAuto(name.trim().length === 0);
    }
  };

  const pickFiles = async () => {
    setPickerError(null);
    try {
      const selection = await open({
        multiple: true,
        filters: [{ name: "MDX 词典", extensions: ["mdx"] }],
      });
      if (!selection) return;
      const picked = (Array.isArray(selection) ? selection : [selection]).filter(
        (item): item is string => typeof item === "string" && item.length > 0,
      );
      if (picked.length === 0) return;
      setPaths((current) => {
        const seen = new Set(current.map(normalisePath));
        const next = [...current];
        for (const item of picked) {
          if (!seen.has(normalisePath(item))) {
            seen.add(normalisePath(item));
            next.push(item);
          }
        }
        return next;
      });
      setFieldErrors((current) => ({ ...current, paths: undefined }));
      if (nameAuto) {
        const first = picked[0];
        if (first) setName(stripMdxExtension(basename(first)));
      }
    } catch (cause: unknown) {
      const message = cause instanceof Error ? cause.message : String(cause);
      setPickerError(`无法打开文件选择器：${message}`);
    }
  };

  const removePath = (path: string) => {
    setPaths((current) => current.filter((item) => item !== path));
  };

  const validateLocal = (): FieldErrors => {
    const errors: FieldErrors = {};
    if (paths.length === 0) {
      errors.paths = "请选择至少一个 .mdx 文件。";
    } else {
      const invalidExtension = paths.find((path) => !/\.mdx$/i.test(path));
      if (invalidExtension) {
        errors.paths = `只支持 .mdx 文件：${basename(invalidExtension)}。`;
      } else {
        const others = existingSources.filter((item) => item.id !== source?.id);
        for (const path of paths) {
          const conflict = others.find(
            (item) =>
              item.type === "local_file" &&
              item.paths.some((existing) => normalisePath(existing) === normalisePath(path)),
          );
          if (conflict) {
            errors.paths = `该文件已被「${conflict.name}」使用：${basename(path)}。`;
            break;
          }
        }
      }
    }
    return errors;
  };

  const validateRemote = (): FieldErrors => {
    const errors: FieldErrors = {};

    const template = urlTemplate.trim();
    if (template.length === 0) {
      errors.urlTemplate = "请输入 URL 模板。";
    } else {
      const placeholders = template.split("{word}").length - 1;
      if (placeholders !== 1) {
        errors.urlTemplate = "URL 模板必须恰好包含一个 {word}。";
      } else if (!hasHttpScheme(template.replace("{word}", "test"))) {
        errors.urlTemplate = "URL 模板只支持 http/https。";
      }
    }

    const header = authHeader.trim();
    const value = authValue;
    if (header.length > 0 !== value.length > 0) {
      errors.auth = "鉴权头名与值需成对填写，或都留空。";
    } else if (/[\r\n]/.test(header) || /[\r\n]/.test(value)) {
      errors.auth = "鉴权头名与值不能包含换行。";
    }

    const timeout = parseInteger(timeoutMs);
    if (timeout === null || timeout < 200 || timeout > 15000) {
      errors.timeout = "超时需为 200–15000 的整数（毫秒）。";
    }

    if (parserMode === "jsonpath") {
      if (expression.trim().length === 0) {
        errors.jsonpath = "请输入 JSONPath 表达式。";
      }
    } else {
      if (scriptPath.trim().length === 0) {
        errors.scriptPath = "请输入脚本路径。";
      }
      if (scriptTimeout.trim().length > 0) {
        const scriptTimeoutValue = parseInteger(scriptTimeout);
        if (
          scriptTimeoutValue === null ||
          scriptTimeoutValue < 50 ||
          scriptTimeoutValue > 1000
        ) {
          errors.scriptTimeout = "脚本超时需为 50–1000 的整数（毫秒），或留空使用默认值。";
        }
      }
    }

    return errors;
  };

  const validate = (): FieldErrors => {
    const errors: FieldErrors = {};
    const trimmedName = name.trim();
    if (trimmedName.length === 0) {
      errors.name = "请输入数据源名称。";
    } else if (trimmedName.length > 80) {
      errors.name = "数据源名称不能超过 80 个字符。";
    }

    const priorityValue = parseInteger(priority);
    if (priorityValue === null || priorityValue < 1 || priorityValue > 9999) {
      errors.priority = "优先级需为 1–9999 的整数。";
    }

    const typeErrors = kind === "local_file" ? validateLocal() : validateRemote();
    return { ...errors, ...typeErrors };
  };

  const buildRemoteConfig = (configName: string): RemoteSourceConfig => ({
    id: mode === "edit" && source ? source.id : "",
    name: configName,
    type: "remote_http",
    url_template: urlTemplate.trim(),
    response_parser:
      parserMode === "jsonpath"
        ? { mode: "jsonpath", expression: expression.trim() }
        : {
            mode: "script",
            path: scriptPath.trim(),
            timeout_ms: parseInteger(scriptTimeout),
          },
    auth_header: authHeader.trim().length > 0 ? authHeader.trim() : null,
    auth_value: authValue.length > 0 ? authValue : null,
    timeout_ms: parseInteger(timeoutMs) ?? 3000,
    enabled,
    priority: parseInteger(priority) ?? 10,
  });

  const buildConfig = (): SourceConfig => {
    const configName = name.trim();
    if (kind === "remote_http") return buildRemoteConfig(configName);
    return {
      id: mode === "edit" && source ? source.id : "",
      name: configName,
      type: "local_file",
      paths,
      enabled,
      priority: parseInteger(priority) ?? 10,
    };
  };

  const focusFirstError = (errors: FieldErrors) => {
    if (errors.name) nameRef.current?.focus();
    else if (errors.paths) fileButtonRef.current?.focus();
    else if (errors.priority) priorityRef.current?.focus();
    else if (errors.urlTemplate) urlRef.current?.focus();
    else if (errors.auth) authHeaderRef.current?.focus();
    else if (errors.timeout) timeoutRef.current?.focus();
    else if (errors.jsonpath) expressionRef.current?.focus();
    else if (errors.scriptPath) scriptPathRef.current?.focus();
    else if (errors.scriptTimeout) scriptTimeoutRef.current?.focus();
  };

  const submit = async () => {
    if (saving) return;
    const errors = validate();
    setFieldErrors(errors);
    if (Object.keys(errors).length > 0) {
      focusFirstError(errors);
      return;
    }

    setSaving(true);
    setSaveError(null);
    try {
      const config = buildConfig();
      if (mode === "edit" && source) {
        await updateSource(source.id, config);
      } else {
        await addSource(config);
      }
      onSaved();
    } catch (cause: unknown) {
      setSaveError(toIpcError(cause));
    } finally {
      setSaving(false);
    }
  };

  const runDraftTest = async () => {
    if (testing || kind !== "remote_http") return;
    const remoteErrors = validateRemote();
    setFieldErrors((current) => ({ ...current, ...remoteErrors }));
    const trimmed = testQuery.trim();
    if (trimmed.length === 0) {
      setTestQueryError("请输入要查询的词。");
      return;
    }
    setTestQueryError(null);
    if (Object.keys(remoteErrors).length > 0) return;

    setTesting(true);
    setTestResult(null);
    setTestError(null);
    try {
      const config = buildRemoteConfig(name.trim() || "远程源");
      setTestResult(await testSourceQuery(config, trimmed));
    } catch (cause: unknown) {
      setTestError(toIpcError(cause));
    } finally {
      setTesting(false);
    }
  };

  const remote = kind === "remote_http";
  const title = mode === "edit" ? "编辑数据源" : "添加数据源";

  return (
    <Modal labelledBy="source-dialog-title" onClose={onClose} initialFocus={nameRef}>
      <form
        noValidate
        onSubmit={(event) => {
          event.preventDefault();
          void submit();
        }}
      >
        <div className="flex items-start justify-between gap-4">
          <div>
            <h2 id="source-dialog-title" className="text-[16px] font-medium">
              {title}
            </h2>
            <p className="mt-1 text-sm text-ink-muted">
              {mode === "edit"
                ? "修改后保存会重新加载该源。"
                : "本地词典读取 .mdx；远程 API 按模板请求并解析响应。"}
            </p>
          </div>
          <button
            type="button"
            onClick={onClose}
            aria-label="关闭对话框"
            className="flex size-8 shrink-0 items-center justify-center rounded-full text-ink-muted transition-colors duration-150 hover:bg-canvas-soft hover:text-ink"
          >
            <svg viewBox="0 0 16 16" aria-hidden="true" className="size-4" fill="none">
              <path d="M4 4l8 8M12 4l-8 8" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" />
            </svg>
          </button>
        </div>

        {saveError ? (
          <div role="alert" className="mt-4 rounded-2xl border border-danger/40 px-4 py-3 text-sm">
            <p className="font-medium text-danger">保存失败</p>
            <p className="mt-1 text-ink-muted">{saveError.message}</p>
            <p className="mt-1 font-mono text-[11px] tracking-[0.6px] text-ink-muted">{saveError.code}</p>
          </div>
        ) : null}

        <div className="mt-5 flex flex-col gap-5">
          <fieldset className="flex flex-col gap-2">
            <legend className="text-sm font-medium">数据源类型</legend>
            <div role="radiogroup" aria-label="数据源类型" className="grid grid-cols-2 gap-2">
              {TYPE_OPTIONS.map((option) => {
                const checked = kind === option.value;
                return (
                  <label
                    key={option.value}
                    className={`flex cursor-pointer items-start gap-2 rounded-xl border px-3 py-2 text-sm transition-colors duration-150 ${
                      checked ? "border-accent" : "border-hairline hover:bg-canvas-soft"
                    }`}
                  >
                    <input
                      type="radio"
                      name="source-type"
                      value={option.value}
                      checked={checked}
                      onChange={() => switchKind(option.value)}
                      className="mt-0.5 size-4 accent-accent"
                    />
                    <span>
                      <span className="block font-medium">{option.label}</span>
                      <span className="mt-0.5 block text-xs text-ink-muted">
                        {option.description}
                      </span>
                    </span>
                  </label>
                );
              })}
            </div>
            {mode === "edit" ? (
              <p className="text-xs text-ink-muted">
                切换类型会清空该类型的字段；保存后该源按新类型重新加载。
              </p>
            ) : null}
          </fieldset>

          <Field
            label="名称"
            id="source-name"
            required
            error={fieldErrors.name ?? null}
            hint="用于列表、测试结果与卡片来源。"
          >
            <input
              ref={nameRef}
              value={name}
              onChange={(event) => {
                setName(event.target.value);
                setNameAuto(false);
                setFieldErrors((current) => ({ ...current, name: undefined }));
              }}
              className="h-10 rounded-lg border border-hairline bg-canvas px-3 text-sm outline-none focus:ring-2 focus:ring-accent focus:ring-offset-1"
              placeholder={remote ? "例如：例词 API" : "例如：牛津高阶"}
            />
          </Field>

          {kind === "local_file" ? (
            <div className="flex flex-col gap-2">
              <span className="text-sm font-medium">
                词典文件<span className="ml-1 text-danger">*</span>
              </span>
              <button
                ref={fileButtonRef}
                type="button"
                onClick={() => void pickFiles()}
                aria-describedby="source-paths-hint source-paths-error"
                className="h-10 self-start rounded-full border border-hairline px-5 text-sm transition-colors duration-150 hover:bg-canvas-soft"
              >
                选择 .mdx 文件
              </button>
              <p id="source-paths-hint" className="text-xs text-ink-muted">
                选择多个文件时按顺序加载；同名 .mdd 图片与音频会自动生效。
              </p>
              {paths.length > 0 ? (
                <ul className="flex flex-col gap-1">
                  {paths.map((path) => (
                    <li
                      key={path}
                      className="flex items-center justify-between gap-3 rounded-xl bg-canvas-soft px-3 py-2"
                    >
                      <span className="min-w-0 truncate font-mono text-xs" title={path}>
                        {basename(path)}
                      </span>
                      <button
                        type="button"
                        onClick={() => removePath(path)}
                        aria-label={`移除 ${basename(path)}`}
                        className="shrink-0 text-xs text-ink-muted transition-colors duration-150 hover:text-danger"
                      >
                        移除
                      </button>
                    </li>
                  ))}
                </ul>
              ) : null}
              {fieldErrors.paths ? (
                <p id="source-paths-error" role="alert" className="text-xs text-danger">
                  {fieldErrors.paths}
                </p>
              ) : null}
              {pickerError ? (
                <p role="alert" className="text-xs text-danger">
                  {pickerError}
                </p>
              ) : null}
            </div>
          ) : (
            <>
              <Field
                label="URL 模板"
                id="source-url-template"
                required
                error={fieldErrors.urlTemplate ?? null}
                hint="必须恰好包含一个 {word}，只支持 http/https。"
              >
                <input
                  ref={urlRef}
                  value={urlTemplate}
                  onChange={(event) => {
                    setUrlTemplate(event.target.value);
                    setFieldErrors((current) => ({ ...current, urlTemplate: undefined }));
                  }}
                  className="h-10 rounded-lg border border-hairline bg-canvas px-3 font-mono text-xs outline-none focus:ring-2 focus:ring-accent focus:ring-offset-1"
                  placeholder="https://api.example.com/lookup?q={word}"
                />
              </Field>

              <div className="grid grid-cols-2 gap-4">
                <Field
                  label="鉴权头名"
                  id="source-auth-header"
                  hint="可选；留空表示不发送。"
                  error={fieldErrors.auth ?? null}
                >
                  <input
                    ref={authHeaderRef}
                    value={authHeader}
                    onChange={(event) => {
                      setAuthHeader(event.target.value);
                      setFieldErrors((current) => ({ ...current, auth: undefined }));
                    }}
                    className="h-10 rounded-lg border border-hairline bg-canvas px-3 font-mono text-xs outline-none focus:ring-2 focus:ring-accent focus:ring-offset-1"
                    placeholder="Authorization"
                  />
                </Field>
                <Field label="鉴权头值" id="source-auth-value" hint="与头名成对填写。">
                  <input
                    type="password"
                    value={authValue}
                    onChange={(event) => {
                      setAuthValue(event.target.value);
                      setFieldErrors((current) => ({ ...current, auth: undefined }));
                    }}
                    className="h-10 rounded-lg border border-hairline bg-canvas px-3 font-mono text-xs outline-none focus:ring-2 focus:ring-accent focus:ring-offset-1"
                    placeholder="Bearer …"
                  />
                </Field>
              </div>

              <Field
                label="请求超时"
                id="source-timeout"
                hint="200–15000 毫秒；默认 3000 毫秒。"
                error={fieldErrors.timeout ?? null}
              >
                <input
                  ref={timeoutRef}
                  inputMode="numeric"
                  value={timeoutMs}
                  onChange={(event) => {
                    setTimeoutMs(event.target.value);
                    setFieldErrors((current) => ({ ...current, timeout: undefined }));
                  }}
                  className="h-10 rounded-lg border border-hairline bg-canvas px-3 text-sm outline-none focus:ring-2 focus:ring-accent focus:ring-offset-1"
                />
              </Field>

              <fieldset className="flex flex-col gap-2">
                <legend className="text-sm font-medium">响应解析方式</legend>
                <div role="radiogroup" aria-label="响应解析方式" className="flex gap-2">
                  {(
                    [
                      { value: "jsonpath", label: "JSONPath 表达式" },
                      { value: "script", label: "脚本" },
                    ] as const
                  ).map((option) => {
                    const checked = parserMode === option.value;
                    return (
                      <label
                        key={option.value}
                        className={`flex cursor-pointer items-center gap-2 rounded-xl border px-3 py-2 text-sm transition-colors duration-150 ${
                          checked ? "border-accent" : "border-hairline hover:bg-canvas-soft"
                        }`}
                      >
                        <input
                          type="radio"
                          name="parser-mode"
                          value={option.value}
                          checked={checked}
                          onChange={() => {
                            setParserMode(option.value);
                            setFieldErrors((current) => ({
                              ...current,
                              jsonpath: undefined,
                              scriptPath: undefined,
                              scriptTimeout: undefined,
                            }));
                          }}
                          className="size-4 accent-accent"
                        />
                        {option.label}
                      </label>
                    );
                  })}
                </div>
              </fieldset>

              {parserMode === "jsonpath" ? (
                <Field
                  label="JSONPath 表达式"
                  id="source-jsonpath"
                  required
                  error={fieldErrors.jsonpath ?? null}
                  hint="例如 $.entries[*]；缺 headword 时使用查询词，缺 definition_html 的条目会被跳过。"
                >
                  <input
                    ref={expressionRef}
                    value={expression}
                    onChange={(event) => {
                      setExpression(event.target.value);
                      setFieldErrors((current) => ({ ...current, jsonpath: undefined }));
                    }}
                    className="h-10 rounded-lg border border-hairline bg-canvas px-3 font-mono text-xs outline-none focus:ring-2 focus:ring-accent focus:ring-offset-1"
                    placeholder={DEFAULT_JSONPATH}
                  />
                </Field>
              ) : (
                <>
                  <div
                    role="note"
                    className="rounded-xl border border-warning/40 bg-canvas-soft px-4 py-3 text-xs"
                  >
                    <p className="font-medium text-warning">只运行可信脚本</p>
                    <p className="mt-1 text-ink-muted">
                      脚本在本机沙箱中执行，可读取响应体与查询词；不要加载来源不明的脚本。
                    </p>
                  </div>
                  <Field
                    label="脚本路径"
                    id="source-script-path"
                    required
                    error={fieldErrors.scriptPath ?? null}
                    hint="相对配置目录，或使用绝对路径。"
                  >
                    <input
                      ref={scriptPathRef}
                      value={scriptPath}
                      onChange={(event) => {
                        setScriptPath(event.target.value);
                        setFieldErrors((current) => ({ ...current, scriptPath: undefined }));
                      }}
                      className="h-10 rounded-lg border border-hairline bg-canvas px-3 font-mono text-xs outline-none focus:ring-2 focus:ring-accent focus:ring-offset-1"
                      placeholder="parsers/example.js"
                    />
                  </Field>
                  <Field
                    label="脚本超时（毫秒）"
                    id="source-script-timeout"
                    hint="50–1000；留空使用默认 200 毫秒。"
                    error={fieldErrors.scriptTimeout ?? null}
                  >
                    <input
                      ref={scriptTimeoutRef}
                      inputMode="numeric"
                      value={scriptTimeout}
                      onChange={(event) => {
                        setScriptTimeout(event.target.value);
                        setFieldErrors((current) => ({ ...current, scriptTimeout: undefined }));
                      }}
                      className="h-10 rounded-lg border border-hairline bg-canvas px-3 text-sm outline-none focus:ring-2 focus:ring-accent focus:ring-offset-1"
                      placeholder="200"
                    />
                  </Field>
                </>
              )}

              <div className="rounded-2xl border border-hairline px-4 py-4">
                <p className="text-sm font-medium">测试草稿配置</p>
                <p className="mt-1 text-xs text-ink-muted">
                  使用当前表单内容查询；不写盘、不缓存。远程失败会显示错误与原始响应。
                </p>
                <div className="mt-3 flex gap-2">
                  <input
                    id="source-test-query"
                    value={testQuery}
                    onChange={(event) => {
                      setTestQuery(event.target.value);
                      setTestQueryError(null);
                    }}
                    aria-label="测试查询词"
                    aria-invalid={testQueryError ? true : undefined}
                    aria-describedby={testQueryError ? "source-test-query-error" : undefined}
                    placeholder="例如：serendipity"
                    className="h-10 min-w-0 flex-1 rounded-lg border border-hairline bg-canvas px-3 text-sm outline-none focus:ring-2 focus:ring-accent focus:ring-offset-1"
                  />
                  <button
                    type="button"
                    onClick={() => void runDraftTest()}
                    disabled={testing}
                    className="h-10 shrink-0 rounded-full border border-hairline px-4 text-sm transition-colors duration-150 hover:bg-canvas-soft disabled:pointer-events-none disabled:opacity-48"
                  >
                    {testing ? "测试中…" : "测试"}
                  </button>
                </div>
                {testQueryError ? (
                  <p id="source-test-query-error" role="alert" className="mt-2 text-xs text-danger">
                    {testQueryError}
                  </p>
                ) : null}
                {testResult || testError ? (
                  <div className="mt-3">
                    <TestResultView
                      result={testResult}
                      error={testError}
                      name={name.trim() || "远程源"}
                    />
                  </div>
                ) : null}
              </div>
            </>
          )}

          <div className="grid grid-cols-2 gap-4">
            <Field
              label="优先级"
              id="source-priority"
              error={fieldErrors.priority ?? null}
              hint="数值越小越优先。"
            >
              <input
                ref={priorityRef}
                inputMode="numeric"
                value={priority}
                onChange={(event) => {
                  setPriority(event.target.value);
                  setFieldErrors((current) => ({ ...current, priority: undefined }));
                }}
                className="h-10 rounded-lg border border-hairline bg-canvas px-3 text-sm outline-none focus:ring-2 focus:ring-accent focus:ring-offset-1"
              />
            </Field>
            <label className="flex items-end gap-2 pb-6 text-sm">
              <input
                type="checkbox"
                checked={enabled}
                onChange={(event) => setEnabled(event.target.checked)}
                className="size-4 accent-accent"
              />
              启用此源
            </label>
          </div>
        </div>

        <div className="mt-6 flex items-center justify-between gap-3 border-t border-hairline pt-4">
          <p className="min-w-0 truncate font-mono text-[11px] tracking-[0.6px] text-ink-muted">
            {remote ? `远程 API · ${parserMode === "jsonpath" ? "JSONPath" : "脚本"}` : ""}
          </p>
          <div className="flex shrink-0 items-center gap-3">
            <button
              type="button"
              onClick={onClose}
              className="h-10 rounded-full border border-hairline px-5 text-sm transition-colors duration-150 hover:bg-canvas-soft"
            >
              取消
            </button>
            <button
              type="submit"
              disabled={saving}
              className="h-10 rounded-full bg-accent px-5 text-sm text-on-accent transition-transform duration-150 active:scale-[0.98] disabled:opacity-50"
            >
              {saving ? "保存中…" : "保存"}
            </button>
          </div>
        </div>
      </form>
    </Modal>
  );
}
