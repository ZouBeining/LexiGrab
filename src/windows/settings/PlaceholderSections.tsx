import type { ReactNode } from "react";

interface SectionScaffoldProps {
  id: string;
  title: string;
  description: string;
  milestone: string;
  items: string[];
  footer?: ReactNode;
}

function SectionScaffold({ id, title, description, milestone, items, footer }: SectionScaffoldProps) {
  return (
    <section aria-labelledby={`${id}-title`} className="flex max-w-[560px] flex-col gap-6">
      <header>
        <h1 id={`${id}-title`} className="text-[26px] font-medium tracking-tight">
          {title}
        </h1>
        <p className="mt-1 text-sm text-ink-muted">{description}</p>
      </header>
      <div className="rounded-3xl bg-canvas-soft px-6 py-6">
        <p className="font-mono text-[11px] tracking-[0.6px] text-ink-muted">
          将在 {milestone} 提供
        </p>
        <p className="mt-2 text-sm">
          该分区暂不放控件，避免保存无效配置。研发进度按里程碑推进。
        </p>
        <ul className="mt-4 flex list-disc flex-col gap-1 pl-5 text-sm text-ink-muted">
          {items.map((item) => (
            <li key={item}>{item}</li>
          ))}
        </ul>
        {footer ? <div className="mt-4">{footer}</div> : null}
      </div>
    </section>
  );
}

/** 语境辅助 is planned for M3; show scope only (no fake controls). */
export function SenseSection() {
  return (
    <SectionScaffold
      id="sense"
      title="语境辅助"
      description="JEV-like 选择器、置信度阈值、LLM 兜底与隐私开关。"
      milestone="M3"
      items={[
        "启用选择器并配置回环地址与超时。",
        "调整低/高置信度阈值，界面标注“模型自评”。",
        "LLM 兜底开关、模型名与 API key 环境变量。",
        "远程端点需二次确认；默认只允许回环。",
      ]}
    />
  );
}

/** 缓存 is planned for M4; show scope only. */
export function CacheSection() {
  return (
    <SectionScaffold
      id="cache"
      title="缓存"
      description="查询缓存条目、占用、TTL 与清理。"
      milestone="M4"
      items={[
        "查看缓存条目数与磁盘占用。",
        "调整正结果与负结果 TTL。",
        "分别清理词典缓存与语境缓存。",
      ]}
    />
  );
}
