//! 生成指定条目数的合成 MDX，用于 T1.10 性能基线。大文件不提交。
//!
//! 运行（建议 release）：
//! `cargo run --release --example gen_large -- 100000 target/perf/large.mdx`
//! 然后：`cargo run --release --bin lexigrab-dev -- bench --config <config> --word word00042 --iterations 100`

use std::path::PathBuf;

use mdictlib::{MdxBuilder, WriteCompression, WriteOptions};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let count: usize = args
        .next()
        .unwrap_or_else(|| "100000".to_string())
        .parse()?;
    let path = PathBuf::from(
        args.next()
            .unwrap_or_else(|| "target/perf/large.mdx".to_string()),
    );
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let options = WriteOptions::new().with_compression(WriteCompression::Zlib);
    let mut builder = MdxBuilder::with_options(options);
    builder.header_attribute("Title", "LexiGrab Synthetic Large Dictionary")?;
    for index in 0..count {
        let headword = format!("word{index:05}");
        let body = format!(
            "<div class=\"entry\"><h1>{headword}</h1><ol><li>synthetic definition one for {headword}</li><li>synthetic definition two for {headword}</li></ol></div>"
        );
        builder.add_entry(&headword, &body)?;
    }
    let file = std::fs::File::create(&path)?;
    let summary = builder.finish(file)?;
    println!(
        "wrote {} entries, {} bytes to {}",
        summary.entries,
        summary.total_bytes,
        path.display()
    );
    Ok(())
}
