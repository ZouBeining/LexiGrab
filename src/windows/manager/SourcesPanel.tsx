export function SourcesPanel() {
  return (
    <section aria-labelledby="sources-title" className="flex max-w-[760px] flex-col gap-6">
      <header className="flex items-start justify-between gap-4">
        <div>
          <h1 id="sources-title" className="text-[26px] font-medium tracking-tight">
            词典源
          </h1>
          <p className="mt-1 text-sm text-ink-muted">本地 MDX 与远程 HTTP 词典都从这里管理。</p>
        </div>
        <button
          type="button"
          disabled
          className="h-10 shrink-0 rounded-full bg-accent px-5 text-sm text-on-accent opacity-48"
        >
          添加源
        </button>
      </header>

      <div className="rounded-3xl bg-block-3 px-6 py-8">
        <p className="text-[16px] font-medium">还没有词典源</p>
        <p className="mt-2 max-w-[420px] text-sm text-ink-muted">
          M1 会在这里接入本地词典与远程词典。M0 只验证划词链路，不需要词典数据。
        </p>
      </div>

      <div className="rounded-3xl border border-hairline">
        <div className="flex items-center justify-between border-b border-hairline px-6 py-3">
          <span className="font-mono text-[11px] tracking-[0.6px] text-ink-muted">源列表</span>
          <span className="font-mono text-[11px] text-ink-muted">0 项</span>
        </div>
        <p className="px-6 py-10 text-center text-sm text-ink-muted">暂无内容</p>
      </div>
    </section>
  );
}
