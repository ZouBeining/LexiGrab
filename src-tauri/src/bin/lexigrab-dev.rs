//! 开发 CLI：不依赖 Tauri 窗口，直接驱动 M1 核心链路。
//!
//! 退出码：0 成功；2 配置/源错误；3 Anki 错误；4 查询无结果。

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;

use clap::{Parser, Subcommand};
use lexigrab_lib::anki::client::AnkiClient;
use lexigrab_lib::anki::mapping::build_note_request;
use lexigrab_lib::core::config::{ConfigLoad, ConfigStore};
use lexigrab_lib::core::entry::{AnkiPayload, Entry};
use lexigrab_lib::core::error::AnkiError;
use lexigrab_lib::core::html;
use lexigrab_lib::core::query_router::QueryRouter;
use lexigrab_lib::core::registry::SourceRegistry;
use lexigrab_lib::core::runtime::{EventSink, RuntimeEvent};

const EXIT_OK: u8 = 0;
const EXIT_CONFIG: u8 = 2;
const EXIT_ANKI: u8 = 3;
const EXIT_EMPTY: u8 = 4;

#[derive(Debug, Parser)]
#[command(name = "lexigrab-dev", about = "LexiGrab M1 开发 CLI", version)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// 查询词典并输出结果
    Lookup {
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        word: String,
        #[arg(long)]
        source: Option<String>,
        #[arg(long)]
        json: bool,
        #[arg(long)]
        html_out: Option<PathBuf>,
    },
    /// 列出配置中的数据源与运行时状态
    Sources {
        #[arg(long)]
        config: PathBuf,
    },
    /// 列出 Anki 牌组
    AnkiDecks {
        #[arg(long)]
        config: PathBuf,
    },
    /// 列出 Anki 模板及其字段
    AnkiModels {
        #[arg(long)]
        config: PathBuf,
    },
    /// 把一条释义写入 Anki
    AddNote {
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        word: String,
        #[arg(long)]
        definition_file: PathBuf,
        #[arg(long)]
        sentence: Option<String>,
        #[arg(long)]
        source: Option<String>,
    },
    /// 查询压测，输出 p50/p95
    Bench {
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        word: String,
        #[arg(long, default_value_t = 100)]
        iterations: u32,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli) {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            eprintln!("{}", error.message);
            ExitCode::from(error.code)
        }
    }
}

struct CliError {
    code: u8,
    message: String,
}

impl CliError {
    fn config(message: impl Into<String>) -> Self {
        Self {
            code: EXIT_CONFIG,
            message: message.into(),
        }
    }

    fn anki(error: &AnkiError) -> Self {
        Self {
            code: EXIT_ANKI,
            message: error.to_string(),
        }
    }
}

fn run(cli: Cli) -> Result<u8, CliError> {
    match cli.command {
        Command::Lookup {
            config,
            word,
            source,
            json,
            html_out,
        } => lookup(&config, &word, source.as_deref(), json, html_out.as_deref()),
        Command::Sources { config } => list_sources(&config),
        Command::AnkiDecks { config } => list_anki_decks(&config),
        Command::AnkiModels { config } => list_anki_models(&config),
        Command::AddNote {
            config,
            word,
            definition_file,
            sentence,
            source,
        } => add_note(&config, &word, &definition_file, sentence, source),
        Command::Bench {
            config,
            word,
            iterations,
        } => bench(&config, &word, iterations),
    }
}

fn load_config(config_path: &Path) -> ConfigLoad {
    let store = ConfigStore::with_config_path(config_path.to_path_buf());
    let load = store.load();
    if let Some(warning) = &load.warning {
        eprintln!("配置警告：{warning}");
    }
    load
}

fn build_registry(load: &ConfigLoad) -> SourceRegistry {
    let base_dir = load
        .config_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    SourceRegistry::from_settings(&load.settings, &base_dir)
}

struct StderrSink;

impl EventSink for StderrSink {
    fn emit(&self, event: RuntimeEvent) {
        if let RuntimeEvent::SourceFailure {
            source_name,
            code,
            message,
            ..
        } = event
        {
            eprintln!("数据源 {source_name} 查询失败 [{code}] {message}");
        }
    }
}

fn lookup(
    config_path: &Path,
    word: &str,
    source_id: Option<&str>,
    json: bool,
    html_out: Option<&Path>,
) -> Result<u8, CliError> {
    let load = load_config(config_path);
    let registry = build_registry(&load);
    if registry.is_empty() {
        return Err(CliError::config("没有启用的本地数据源，请检查配置"));
    }
    let router = QueryRouter::new(&registry, &load.settings);
    let source_ids = source_id.map(|id| vec![id.to_string()]);
    let entries = router
        .lookup(word, source_ids.as_deref(), &StderrSink)
        .map_err(|error| {
            CliError::config(format!("查询失败 [{}] {}", error.code, error.message))
        })?;

    if entries.is_empty() {
        eprintln!("未查询到结果：{word}");
        return Ok(EXIT_EMPTY);
    }

    if let Some(path) = html_out {
        let document = render_html_document(word, &entries);
        std::fs::write(path, document)
            .map_err(|error| CliError::config(format!("写入 HTML 失败: {error}")))?;
        eprintln!("HTML 已写入 {}", path.display());
    }

    if json {
        let serialized = serde_json::to_string_pretty(&entries)
            .map_err(|error| CliError::config(format!("序列化失败: {error}")))?;
        println!("{serialized}");
    } else {
        for (index, entry) in entries.iter().enumerate() {
            if index > 0 {
                println!();
            }
            println!("{}  [{}]", entry.headword, entry.source_name);
            if let Some(phonetic) = &entry.phonetic {
                println!("{phonetic}");
            }
            println!("{}", html_to_text(&entry.definition_html));
            if !entry.resources.is_empty() {
                println!("资源：{} 项", entry.resources.len());
            }
        }
    }
    Ok(EXIT_OK)
}

fn render_html_document(word: &str, entries: &[Entry]) -> String {
    let mut document =
        String::from("<!doctype html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n");
    document.push_str(&format!("<title>{}</title>\n", escape_html_text(word)));
    document.push_str("</head>\n<body>\n");
    for entry in entries {
        document.push_str("<article class=\"entry\">\n");
        document.push_str(&format!(
            "<h1>{}</h1>\n<div class=\"source\">{}</div>\n",
            escape_html_text(&entry.headword),
            escape_html_text(&entry.source_name)
        ));
        document.push_str(&entry.definition_html);
        document.push_str("\n</article>\n<hr>\n");
    }
    document.push_str("</body>\n</html>\n");
    document
}

fn escape_html_text(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn html_to_text(fragment: &str) -> String {
    let document = scraper::Html::parse_fragment(fragment);
    let mut parts = Vec::new();
    for text in document.root_element().text() {
        let trimmed = text.trim();
        if !trimmed.is_empty() {
            parts.push(trimmed.to_string());
        }
    }
    parts.join(" ")
}

fn list_sources(config_path: &Path) -> Result<u8, CliError> {
    let load = load_config(config_path);
    let registry = build_registry(&load);
    for state in registry.states() {
        println!(
            "{}\t{}\tenabled={}\tpriority={}\t{}",
            state.id,
            state.name,
            state.enabled,
            state.priority,
            describe_status(&state.status)
        );
    }
    for source in &load.settings.sources {
        if load.settings.sources.iter().any(|entry| {
            entry.id == source.id
                && !matches!(
                    entry.kind,
                    lexigrab_lib::core::entry::SourceKind::LocalFile { .. }
                )
        }) {
            println!("{}\t{}\t远程源（M1 不打开）", source.id, source.name);
        }
        if !source.enabled {
            println!("{}\t{}\t禁用", source.id, source.name);
        }
    }
    Ok(EXIT_OK)
}

fn describe_status(status: &lexigrab_lib::sources::SourceStatus) -> String {
    match status {
        lexigrab_lib::sources::SourceStatus::Loading => "loading".to_string(),
        lexigrab_lib::sources::SourceStatus::Ready { entry_count } => {
            format!("ready entries={entry_count}")
        }
        lexigrab_lib::sources::SourceStatus::Failed { message } => {
            format!("failed: {message}")
        }
    }
}

fn anki_client(load: &ConfigLoad) -> Result<AnkiClient, CliError> {
    AnkiClient::new(load.settings.anki.endpoint.clone()).map_err(|error| {
        print_anki_guidance();
        CliError::anki(&error)
    })
}

fn ensure_anki_running(load: &ConfigLoad) -> Result<AnkiClient, CliError> {
    let client = anki_client(load)?;
    match client.version() {
        Ok(_) => Ok(client),
        Err(error) => {
            print_anki_guidance();
            Err(CliError::anki(&error))
        }
    }
}

fn print_anki_guidance() {
    eprintln!("请确认 Anki 正在运行，且已安装并启用 AnkiConnect 插件。");
    eprintln!("默认端点：http://127.0.0.1:8765；可在配置 [anki] endpoint 中修改。");
}

fn list_anki_decks(config_path: &Path) -> Result<u8, CliError> {
    let load = load_config(config_path);
    let client = ensure_anki_running(&load)?;
    let decks = client
        .deck_names()
        .map_err(|error| CliError::anki(&error))?;
    for deck in decks {
        println!("{deck}");
    }
    Ok(EXIT_OK)
}

fn list_anki_models(config_path: &Path) -> Result<u8, CliError> {
    let load = load_config(config_path);
    let client = ensure_anki_running(&load)?;
    let models = client
        .model_names()
        .map_err(|error| CliError::anki(&error))?;
    for model in models {
        match client.model_field_names(&model) {
            Ok(fields) => println!("{model}\t{}", fields.join(", ")),
            Err(error) => println!("{model}\t<字段读取失败: {error}>"),
        }
    }
    Ok(EXIT_OK)
}

fn add_note(
    config_path: &Path,
    word: &str,
    definition_file: &Path,
    sentence: Option<String>,
    source: Option<String>,
) -> Result<u8, CliError> {
    let load = load_config(config_path);
    let raw = std::fs::read_to_string(definition_file)
        .map_err(|error| CliError::config(format!("读取释义文件失败: {error}")))?;
    let definition_html =
        html::sanitize(&raw).map_err(|error| CliError::config(error.to_string()))?;
    let payload = AnkiPayload::new(
        word,
        definition_html,
        sentence,
        source.unwrap_or_else(|| "未知来源".to_string()),
    );
    let client = ensure_anki_running(&load)?;
    let fields = client
        .model_field_names(&load.settings.anki.model)
        .map_err(|error| {
            print_anki_guidance();
            CliError::anki(&error)
        })?;
    let request = build_note_request(&load.settings.anki, &payload, &fields)
        .map_err(|error| CliError::anki(&error))?;
    let note_id = client
        .add_note(&request)
        .map_err(|error| CliError::anki(&error))?;
    println!("{note_id}");
    Ok(EXIT_OK)
}

fn bench(config_path: &Path, word: &str, iterations: u32) -> Result<u8, CliError> {
    let load = load_config(config_path);
    let open_started = Instant::now();
    let registry = build_registry(&load);
    let open_ms = open_started.elapsed().as_secs_f64() * 1000.0;
    if registry.is_empty() {
        return Err(CliError::config("没有启用的本地数据源"));
    }
    let router = QueryRouter::new(&registry, &load.settings);
    let iterations = iterations.max(1);
    let mut durations = Vec::with_capacity(iterations as usize);
    let mut entries_total = 0usize;
    for _ in 0..iterations {
        let started = Instant::now();
        let entries = router.lookup(word, None, &StderrSink).map_err(|error| {
            CliError::config(format!("查询失败 [{}] {}", error.code, error.message))
        })?;
        durations.push(started.elapsed().as_secs_f64() * 1000.0);
        entries_total += entries.len();
    }
    durations.sort_by(|left, right| left.total_cmp(right));
    let p50 = percentile(&durations, 0.50);
    let p95 = percentile(&durations, 0.95);
    let mean = durations.iter().sum::<f64>() / durations.len() as f64;
    println!(
        "iterations={iterations} entries_total={entries_total} open_ms={open_ms:.1} p50_ms={p50:.2} p95_ms={p95:.2} mean_ms={mean:.2}"
    );
    Ok(EXIT_OK)
}

fn percentile(sorted: &[f64], ratio: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let rank = (sorted.len() as f64 * ratio).ceil() as usize;
    let index = rank.saturating_sub(1).min(sorted.len() - 1);
    sorted.get(index).copied().unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use lexigrab_lib::core::entry::ResourceRef;

    #[test]
    fn html_to_text_drops_tags_and_collapses_space() {
        assert_eq!(html_to_text("<p>hello <b>world</b></p>"), "hello world");
    }

    #[test]
    fn html_document_keeps_rewritten_uri() {
        let entry = Entry {
            headword: "hello".to_string(),
            definition_html: "<p><img src=\"mdx-res://local-1/img/a.png\"></p>".to_string(),
            phonetic: None,
            resources: vec![ResourceRef::Local {
                dict_id: "local-1".to_string(),
                path: "img/a.png".to_string(),
            }],
            source_name: "Mini".to_string(),
        };
        let document = render_html_document("hello", &[entry]);
        assert!(document.contains("mdx-res://local-1/img/a.png"));
        assert!(document.contains("<h1>hello</h1>"));
    }

    #[test]
    fn percentile_uses_nearest_rank() {
        let values = [1.0, 2.0, 3.0, 4.0];
        assert_eq!(percentile(&values, 0.5), 2.0);
        assert_eq!(percentile(&values, 0.95), 4.0);
        assert_eq!(percentile(&[], 0.5), 0.0);
    }
}
