// SPDX-License-Identifier: AGPL-3.0-only
//! 生成 M1 测试夹具。提交生成的二进制夹具；CI 不运行本生成器。
//!
//! 运行：`cargo run --example gen_fixtures`（工作目录 src-tauri）
//! 说明：原计划用 Python mdict-utils；本机改为用 mdictlib 自带的写入器，
//! 避免额外 Python 依赖，产物同样是标准 MDX v2.0 / MDD 文件。

use std::path::PathBuf;

use mdictlib::{MddBuilder, MddFile, MdxBuilder, MdxFile, WriteCompression, WriteOptions};

/// 1x1 透明 PNG。
const MINI_PNG: &[u8] = &[
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4,
    0x89, 0x00, 0x00, 0x00, 0x0A, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0x00, 0x01, 0x00, 0x00,
    0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE,
    0x42, 0x60, 0x82,
];

/// 音频占位字节（不是真实音频，只用于验证资源读取与 MIME）。
const MINI_MP3_PLACEHOLDER: &[u8] = b"ID3\x04\x00\x00\x00\x00\x00\x00LexiGrab audio placeholder";

const HELLO_HTML: &str = r#"<div class="entry">
<h1>hello</h1>
<span class="pos">interjection</span>
<ol>
<li>used as a greeting: <b>Hello!</b> she said.</li>
<li>used to express surprise: <i>Hello</i>, what's this?</li>
</ol>
<p><img src="img/hello.png" alt="hello"> <audio src="audio/hello.mp3" controls></audio></p>
</div>"#;

const WORLD_HTML: &str = r#"<div class="entry"><h1>world</h1><p>The earth, together with all of its countries and peoples.</p></div>"#;

const LEXIGRAB_HTML: &str = r#"<div class="entry">
<h1>lexigrab</h1>
<ul>
<li>a demo word for the LexiGrab test fixture</li>
<li>grab words from any application</li>
<li>save them to Anki with context<br>and source</li>
</ul>
</div>"#;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let fixture_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    std::fs::create_dir_all(&fixture_dir)?;
    generate(&fixture_dir)?;
    println!("fixtures written to {}", fixture_dir.display());
    Ok(())
}

fn generate(dir: &std::path::Path) -> Result<(), Box<dyn std::error::Error>> {
    let options = WriteOptions::new().with_compression(WriteCompression::Zlib);

    let mut mdx = MdxBuilder::with_options(options.clone());
    mdx.header_attribute("Title", "LexiGrab Mini Fixture")?;
    mdx.header_attribute("Description", "Synthetic fixture for LexiGrab tests")?;
    mdx.add_entry("hello", HELLO_HTML)?;
    mdx.add_entry("world", WORLD_HTML)?;
    mdx.add_entry("lexigrab", LEXIGRAB_HTML)?;
    let mdx_path = dir.join("mini.mdx");
    let summary = mdx.finish(std::fs::File::create(&mdx_path)?)?;
    println!("mini.mdx entries={}", summary.entries);

    let mut mdd = MddBuilder::with_options(options);
    mdd.header_attribute("Title", "LexiGrab Mini Fixture Resources")?;
    mdd.add_resource("img/hello.png", MINI_PNG)?;
    mdd.add_resource("audio/hello.mp3", MINI_MP3_PLACEHOLDER)?;
    let mdd_path = dir.join("mini.mdd");
    let summary = mdd.finish(std::fs::File::create(&mdd_path)?)?;
    println!("mini.mdd entries={}", summary.entries);

    // 截断文件：只保留前 100 字节，用于验证打开失败路径。
    let bytes = std::fs::read(&mdx_path)?;
    let broken_len = bytes.len().min(100);
    std::fs::write(dir.join("broken.mdx"), &bytes[..broken_len])?;

    verify(&mdx_path, &mdd_path)?;
    Ok(())
}

fn verify(
    mdx_path: &std::path::Path,
    mdd_path: &std::path::Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let mdx = MdxFile::open(mdx_path)?;
    let record = mdx.lookup("hello")?.ok_or("fixture should contain hello")?;
    if !record.text().contains("used as a greeting") {
        return Err("hello fixture content mismatch".into());
    }
    println!("fixture check: hello resolved, {} entries", mdx.len());
    let mdd = MddFile::open(mdd_path)?;
    let resource = mdd
        .lookup("img/hello.png")?
        .ok_or("fixture should contain image")?;
    if !resource.bytes().starts_with(b"\x89PNG") {
        return Err("image resource is not a PNG".into());
    }
    println!("fixture check: image {} bytes", resource.bytes().len());
    Ok(())
}
