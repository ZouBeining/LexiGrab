// SPDX-License-Identifier: AGPL-3.0-only
//! 语境辅助状态机。契约见 docs/BACKEND_STRUCTURE.md §7.7。
//!
//! 顺序固定：
//! 1. 提取候选（`sense.enabled = false` 时也提取，供 UI 展示全部义项）；
//! 2. 没有原句 → `no_context`，不调用选择器；
//! 3. 已缓存且能反序列化的成功结果直接返回（仅替换 query_id/headword）；
//! 4. 有候选才调用选择器，按阈值分档；没有候选直接 `low_confidence`；
//! 5. 仅 `low_confidence` 且 LLM 启用时才调用 LLM，选择器 `unavailable` 不自动兜底；
//! 6. 成功结果写 `sense_cache`，写失败只告警。
//!
//! 日志只记录状态与耗时，不记录单词/原句。

use std::time::Instant;

use serde::Deserialize;
use uuid::Uuid;

use super::llm::{HttpLlm, LlmGenerator, PROMPT_VERSION};
use super::selector::{HttpSelector, SelectorReply, SenseError, SenseSelector};
use super::{AiDefinition, SenseCandidate, SenseResolution, SenseStatus};
use crate::core::config::{LlmSettings, SenseSettings, Settings};
use crate::core::entry::Entry;
use crate::db::cache::CacheStore;
use crate::db::fingerprint::CONTEXT_NAMESPACE;
use crate::db::normalize_key;
use crate::parsers::sense::{extract_senses, MAX_SENSES};

/// `sense_cache` 的 uuid v5 domain，与 saved_cards 的 sentence/definition 分开。
const CACHE_DOMAIN: &str = "sense";

/// 一次语境辅助的输入。
pub struct SenseInput<'a> {
    pub query_id: &'a str,
    pub headword: &'a str,
    pub sentence: Option<&'a str>,
    pub entries: &'a [Entry],
    pub settings: &'a Settings,
    pub cache: &'a CacheStore,
}

/// 使用真实 HTTP 选择器与 LLM 的入口。
pub fn resolve(input: SenseInput<'_>) -> SenseResolution {
    resolve_with(input, &HttpSelector, &HttpLlm)
}

/// 可注入选择器与 LLM 的入口；状态机本身不依赖网络。
pub fn resolve_with(
    input: SenseInput<'_>,
    selector: &dyn SenseSelector,
    llm: &dyn LlmGenerator,
) -> SenseResolution {
    let started = Instant::now();
    let max_senses = input.settings.sense.max_senses;
    let candidates = collect_candidates(input.entries, max_senses);

    let mut resolution = SenseResolution {
        query_id: input.query_id.to_string(),
        headword: input.headword.to_string(),
        status: SenseStatus::Disabled,
        selected_sense_id: None,
        confidence: None,
        selector_model: None,
        ai: None,
        error: None,
        candidates,
    };

    if !input.settings.sense.enabled {
        log_completion(&resolution.status, started);
        return resolution;
    }

    let Some(sentence) = input
        .sentence
        .map(str::trim)
        .filter(|sentence| !sentence.is_empty())
    else {
        resolution.status = SenseStatus::NoContext;
        log_completion(&resolution.status, started);
        return resolution;
    };

    let context_key = context_key(
        input.headword,
        sentence,
        &resolution.candidates,
        &input.settings.llm,
    );
    if let Some(cached) = read_cached(input.cache, &context_key) {
        resolution.status = cached.status;
        resolution.selected_sense_id = cached.selected_sense_id;
        resolution.confidence = cached.confidence;
        resolution.selector_model = cached.selector_model;
        resolution.ai = cached.ai;
        resolution.error = cached.error;
        log_completion(&resolution.status, started);
        return resolution;
    }

    if resolution.candidates.is_empty() {
        // 没有候选也无从选择；仍允许 LLM 给出兜底释义。
        resolution.status = SenseStatus::LowConfidence;
    } else {
        match selector.select(
            &input.settings.sense,
            input.headword,
            sentence,
            &resolution.candidates,
        ) {
            Ok(reply) => apply_selector_reply(&mut resolution, reply, &input.settings.sense),
            Err(error) => apply_selector_error(&mut resolution, error),
        }
    }

    // 选择器不可达不自动调用 LLM；只有低置信度才兜底。
    if resolution.status == SenseStatus::LowConfidence && input.settings.llm.enabled {
        match llm.generate(
            &input.settings.llm,
            input.headword,
            sentence,
            &resolution.candidates,
            resolution.selected_sense_id.as_deref(),
        ) {
            Ok(ai) => resolution.ai = Some(ai),
            Err(error) => {
                resolution.error = Some(format!("{}: {}", error.code(), error.message()));
            }
        }
    }

    if resolution.status.is_cacheable() {
        write_cached(input.cache, &context_key, &resolution, &input.settings.llm);
    }

    log_completion(&resolution.status, started);
    resolution
}

/// 逐个 Entry 提取候选并截断到 `max_senses`（每个 Entry 的 id 前缀保证唯一）。
fn collect_candidates(entries: &[Entry], max_senses: u32) -> Vec<SenseCandidate> {
    let limit = (max_senses as usize).clamp(1, MAX_SENSES);
    let mut candidates = Vec::new();
    for (entry_index, entry) in entries.iter().enumerate() {
        if candidates.len() >= limit {
            break;
        }
        let remaining = limit - candidates.len();
        candidates.extend(extract_senses(entry, entry_index, remaining));
    }
    candidates
}

/// 应用选择器结果：非法/缺失 id 一律低置信度，选中 id 必须属于候选。
fn apply_selector_reply(
    resolution: &mut SenseResolution,
    reply: SelectorReply,
    settings: &SenseSettings,
) {
    let SelectorReply {
        selected_id,
        confidence,
        model,
        distribution: _,
    } = reply;

    resolution.selector_model = model;
    let confidence = confidence
        .filter(|value| value.is_finite())
        .unwrap_or(0.0)
        .clamp(0.0, 1.0);
    resolution.confidence = Some(confidence);

    let selected = selected_id.filter(|id| {
        resolution
            .candidates
            .iter()
            .any(|candidate| &candidate.id == id)
    });
    match selected {
        Some(id) => {
            resolution.status = classify(
                confidence,
                settings.low_confidence,
                settings.high_confidence,
            );
            resolution.selected_sense_id = Some(id);
        }
        None => {
            resolution.status = SenseStatus::LowConfidence;
            resolution.selected_sense_id = None;
        }
    }
}

/// 选择器失败：Disabled/NoContext 不适用，其余按 `unavailable` 处理且不自动调 LLM。
fn apply_selector_error(resolution: &mut SenseResolution, error: SenseError) {
    match error {
        SenseError::Disabled => resolution.status = SenseStatus::Disabled,
        SenseError::NoContext => resolution.status = SenseStatus::NoContext,
        other => {
            resolution.error = Some(format!("{}: {}", other.code(), other.message()));
            resolution.status = SenseStatus::Unavailable;
        }
    }
}

/// 阈值分档：`>= high` 选中；`> low` 模糊；`<= low` 低置信度。
fn classify(confidence: f32, low: f32, high: f32) -> SenseStatus {
    if confidence >= high {
        SenseStatus::Selected
    } else if confidence > low {
        SenseStatus::Ambiguous
    } else {
        SenseStatus::LowConfidence
    }
}

/// 缓存内容的读取结构：不含 candidates（缓存键已包含候选 id 与定义文本）。
#[derive(Debug, Deserialize)]
struct CachedResolution {
    status: SenseStatus,
    #[serde(default)]
    selected_sense_id: Option<String>,
    #[serde(default)]
    confidence: Option<f32>,
    #[serde(default)]
    selector_model: Option<String>,
    #[serde(default)]
    ai: Option<AiDefinition>,
    #[serde(default)]
    error: Option<String>,
}

/// 读缓存；过期、坏 JSON、非成功状态都按 miss 处理，不影响主流程。
fn read_cached(cache: &CacheStore, context_key: &str) -> Option<CachedResolution> {
    let raw = match cache.sense_get(context_key) {
        Ok(Some(raw)) => raw,
        Ok(None) => return None,
        Err(error) => {
            tracing::warn!(error = %error, "读取语境缓存失败");
            return None;
        }
    };
    match serde_json::from_str::<CachedResolution>(&raw) {
        Ok(cached) if cached.status.is_cacheable() => Some(cached),
        Ok(_) => None,
        Err(error) => {
            tracing::warn!(error = %error, "语境缓存内容无法解析，按未命中处理");
            None
        }
    }
}

/// 写缓存；失败只告警，不改变返回结果。
fn write_cached(
    cache: &CacheStore,
    context_key: &str,
    resolution: &SenseResolution,
    settings: &LlmSettings,
) {
    let ttl_secs = i64::from(settings.cache_ttl_hours.max(1)).saturating_mul(3600);
    let json = match serde_json::to_string(resolution) {
        Ok(json) => json,
        Err(error) => {
            tracing::warn!(error = %error, "序列化语境缓存失败");
            return;
        }
    };
    if let Err(error) = cache.sense_put(context_key, &json, ttl_secs) {
        tracing::warn!(error = %error, "写入语境缓存失败");
    }
}

/// 计算缓存键：uuid v5(`sense:<word>|<sentence>|<senses>|<model>|<prompt_version>`)。
fn context_key(
    word: &str,
    sentence: &str,
    candidates: &[SenseCandidate],
    settings: &LlmSettings,
) -> String {
    let senses = candidates
        .iter()
        .map(|candidate| {
            format!(
                "{}:{}",
                candidate.id,
                normalize_key(&candidate.definition_text)
            )
        })
        .collect::<Vec<_>>()
        .join("|");
    let canonical = format!(
        "{}|{}|{}|{}|{}",
        normalize_key(word),
        normalize_key(sentence),
        senses,
        settings.model.trim(),
        PROMPT_VERSION
    );
    Uuid::new_v5(
        &CONTEXT_NAMESPACE,
        format!("{CACHE_DOMAIN}:{canonical}").as_bytes(),
    )
    .to_string()
}

/// 只记录状态与耗时，不记录单词/原句。
fn log_completion(status: &SenseStatus, started: Instant) {
    tracing::debug!(
        status = ?status,
        elapsed_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
        "语境辅助完成"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_key_is_stable_and_model_sensitive() {
        let candidates = vec![SenseCandidate {
            id: "e0-s1".to_string(),
            entry_index: 0,
            source_name: "Mini".to_string(),
            headword: "bank".to_string(),
            part_of_speech: Some("noun".to_string()),
            definition_text: "financial institution".to_string(),
            definition_html: "<li>financial institution</li>".to_string(),
        }];
        let first = LlmSettings {
            model: "model-a".to_string(),
            ..LlmSettings::default()
        };
        let second = LlmSettings {
            model: "model-b".to_string(),
            ..LlmSettings::default()
        };
        let key = context_key(" Bank ", "He sat on the bank.", &candidates, &first);
        assert_eq!(
            key,
            context_key("bank", "  he sat on the bank.  ", &candidates, &first)
        );
        assert_ne!(
            key,
            context_key("bank", "He sat on the bank.", &candidates, &second)
        );
    }

    #[test]
    fn classify_handles_threshold_boundaries() {
        assert_eq!(classify(0.72, 0.55, 0.72), SenseStatus::Selected);
        assert_eq!(classify(0.73, 0.55, 0.72), SenseStatus::Selected);
        assert_eq!(classify(0.56, 0.55, 0.72), SenseStatus::Ambiguous);
        assert_eq!(classify(0.55, 0.55, 0.72), SenseStatus::LowConfidence);
        assert_eq!(classify(0.0, 0.55, 0.72), SenseStatus::LowConfidence);
    }
}
