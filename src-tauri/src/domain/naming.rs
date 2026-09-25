//! How a name becomes a file name: what an exported artifact may be called on disk, what a virtual
//! texture layer is called, and which GTS file belongs to a page file.

// Sanitize a file/directory name: strip Windows-forbidden and control characters, cap the length
pub(crate) fn sanitize_file_name(raw: &str) -> String {
    let cleaned: String = raw
        .chars()
        .filter(|c| !c.is_control())
        .map(replace_forbidden)
        .collect();

    let trimmed = cleaned.trim().trim_matches('.').to_string();
    if trimmed.is_empty() {
        "asset".to_string()
    } else {
        trimmed.chars().take(80).collect()
    }
}

// Windows-forbidden and path-separating characters become underscores, so nothing here can name a
// file outside the directory it is written to
fn replace_forbidden(c: char) -> char {
    if matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*') {
        '_'
    } else {
        c
    }
}

// The extractor calls the base layer `BaseMap` while the engine calls it Albedo; the export follows
// the engine, and the other two layers just drop the trailing `Map` (`NormalMap` → `Normal`)
pub(crate) fn export_layer_name(layer: &str) -> &str {
    match layer {
        "BaseMap" => "Albedo",
        other => other.trim_end_matches("Map"),
    }
}

// GTP path → GTS path: strip the trailing `_<32 hex digits>` and swap the extension
// (`Generated/Public/VirtualTextures/Albedo_Normal_Physical_5_<hash>.gtp` →
// `Generated/Public/VirtualTextures/Albedo_Normal_Physical_5.gts`). What remains is the tile set index,
// so every page file of that set derives the same GTS, which is how one GTS comes to serve many GTPs
pub(crate) fn derive_gts_path(gtp_path: &str) -> String {
    // Split off first and put back at the end: only the file name loses its hash suffix, while the
    // directory has to survive into the result (a GTS sits beside its page file)
    let (dir, name) = gtp_path.rsplit_once('/').unwrap_or(("", gtp_path));
    let stem = strip_hash_suffix(strip_gtp_extension(name));

    if dir.is_empty() {
        format!("{stem}.gts")
    } else {
        format!("{dir}/{stem}.gts")
    }
}

// A page file name without its extension; `name` itself when it ends in neither spelling
fn strip_gtp_extension(name: &str) -> &str {
    name.strip_suffix(".gtp")
        .or_else(|| name.strip_suffix(".GTP"))
        .unwrap_or(name)
}

// A tile set index without its trailing `_<32 hex digits>`: only the index names the GTS, so any other
// trailing word is part of it and stays
fn strip_hash_suffix(stem: &str) -> &str {
    let stripped = stem.rfind('_').filter(|pos| {
        let suffix = &stem[pos + 1..];
        suffix.len() == 32 && suffix.chars().all(|c| c.is_ascii_hexdigit())
    });
    stripped.map_or(stem, |pos| &stem[..pos])
}
