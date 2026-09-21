//! Seti-based file identifiers, independent of GPUI so the resolver can be tested alone.
//! See assets/icons/seti/NOTICE for the pinned sources and intentional palette changes.

use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct FileIcon {
    pub(crate) asset: &'static str,
    dark_color: u32,
}

impl FileIcon {
    pub(crate) fn color(self, light: bool) -> u32 {
        if !light {
            return self.dark_color;
        }
        // Keep the unknown-file fallback legible on light surfaces. For colored
        // glyphs, use VS Code Seti's 90%-per-channel light-theme adjustment.
        if self == DEFAULT {
            return 0x6d8086;
        }
        let channel = |shift: u32| (((self.dark_color >> shift) & 0xff) * 9 + 5) / 10;
        (channel(16) << 16) | (channel(8) << 8) | channel(0)
    }
}

macro_rules! define_icons {
    ($($id:ident => ($file:literal, $color:literal)),* $(,)?) => {
        $(const $id: FileIcon = FileIcon {
            asset: concat!("icons/seti/", $file, ".svg"),
            dark_color: $color,
        };)*

        #[cfg(test)]
        const ASSETS: &[(&str, &str)] = &[
            $(($id.asset, include_str!(concat!("../../../assets/icons/seti/", $file, ".svg")))),*
        ];
    };
}

define_icons! {
    DEFAULT => ("default", 0xd4d7d6),
    RUST => ("rust", 0xe37933),
    PYTHON => ("python", 0x519aba),
    JAVASCRIPT => ("javascript", 0xcbcb41),
    TYPESCRIPT => ("typescript", 0x519aba),
    REACT => ("react", 0x519aba),
    JSON => ("json", 0xcbcb41),
    YAML => ("yml", 0xa074c4),
    MARKDOWN => ("markdown", 0x519aba),
    HTML => ("html", 0xe37933),
    CSS => ("css", 0x519aba),
    C => ("c", 0x519aba),
    CPP => ("cpp", 0x519aba),
    CONFIG => ("config", 0xa074c4),
    GIT => ("git", 0xcc3e44),
    DOCKER => ("docker", 0x519aba),
    IMAGE => ("image", 0xa074c4),
    PDF => ("pdf", 0xcc3e44),
    ARCHIVE => ("zip", 0xe37933),
    DATABASE => ("db", 0xf55385),
    SHELL => ("shell", 0x8dc149),
    VUE => ("vue", 0x8dc149),
}

pub(crate) fn for_path(path: &Path) -> FileIcon {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();

    // Basenames take precedence over suffixes, including extensionless dotfiles.
    match name.as_str() {
        "cargo.toml" | "cargo.lock" => return RUST,
        "dockerfile" | "containerfile" => return DOCKER,
        ".gitignore" | ".gitattributes" | ".gitmodules" | ".gitconfig" | ".gitkeep"
        | "commit_editmsg" | "merge_msg" => return GIT,
        "makefile" | "gnumakefile" | ".env" | ".editorconfig" | ".npmrc" | ".yarnrc"
        | ".prettierrc" | ".eslintrc" => return CONFIG,
        ".bashrc" | ".bash_profile" | ".zshrc" | ".zprofile" | ".profile" => return SHELL,
        "tsconfig.json" => return TYPESCRIPT,
        "jsconfig.json" => return JAVASCRIPT,
        _ => {}
    }
    if name.starts_with("dockerfile.") || name.starts_with("containerfile.") {
        return DOCKER;
    }
    if name.starts_with(".env.") {
        return CONFIG;
    }
    // Match complete compound suffixes, not arbitrary substrings in a path.
    for (suffix, icon) in [
        (".js.map", JAVASCRIPT),
        (".mjs.map", JAVASCRIPT),
        (".cjs.map", JAVASCRIPT),
        (".ts.map", TYPESCRIPT),
        (".css.map", CSS),
    ] {
        if name.ends_with(suffix) {
            return icon;
        }
    }

    // Inspect the extension separately: a non-UTF-8 stem can still have a valid
    // extension on Unix. No file IO, MIME sniffing, or runtime asset downloads.
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    match extension.as_str() {
        "rs" => RUST,
        "py" | "pyw" | "pyi" | "ipynb" => PYTHON,
        "js" | "mjs" | "cjs" => JAVASCRIPT,
        "ts" | "mts" | "cts" => TYPESCRIPT,
        "jsx" | "tsx" => REACT,
        "json" | "jsonc" | "jsonl" => JSON,
        "yaml" | "yml" => YAML,
        "md" | "markdown" | "mdx" => MARKDOWN,
        "html" | "htm" | "xml" | "xhtml" => HTML,
        "css" | "scss" | "sass" | "less" => CSS,
        "c" => C,
        "h" => FileIcon {
            dark_color: 0xa074c4,
            ..C
        },
        "cpp" | "cc" | "cxx" | "c++" => CPP,
        "hpp" | "hh" | "hxx" | "h++" => FileIcon {
            dark_color: 0xa074c4,
            ..CPP
        },
        "toml" | "ini" | "cfg" | "conf" | "config" | "properties" | "lock" => CONFIG,
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "svg" | "ico" | "tif" | "tiff"
        | "avif" | "heic" => IMAGE,
        "pdf" => PDF,
        "zip" | "tar" | "gz" | "tgz" | "bz2" | "xz" | "7z" | "rar" | "zst" => ARCHIVE,
        "sql" | "db" | "sqlite" | "sqlite3" => DATABASE,
        "sh" | "bash" | "zsh" | "fish" | "ps1" | "bat" | "cmd" => SHELL,
        "vue" => VUE,
        _ => DEFAULT,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn common_formats_have_distinct_identifiers() {
        for (name, expected) in [
            ("main.rs", RUST),
            ("main.py", PYTHON),
            ("types.pyi", PYTHON),
            ("index.js", JAVASCRIPT),
            ("index.mjs", JAVASCRIPT),
            ("index.cjs", JAVASCRIPT),
            ("index.ts", TYPESCRIPT),
            ("types.d.ts", TYPESCRIPT),
            ("index.mts", TYPESCRIPT),
            ("index.cts", TYPESCRIPT),
            ("App.jsx", REACT),
            ("App.tsx", REACT),
            ("package.json", JSON),
            ("settings.jsonc", JSON),
            ("events.jsonl", JSON),
            ("ci.yml", YAML),
            ("ci.yaml", YAML),
            ("README.md", MARKDOWN),
            ("notes.markdown", MARKDOWN),
            ("index.html", HTML),
            ("style.css", CSS),
            ("main.c", C),
            ("main.cpp", CPP),
            ("settings.toml", CONFIG),
            ("image.png", IMAGE),
            ("image.svg", IMAGE),
            ("manual.pdf", PDF),
            ("source.tar.gz", ARCHIVE),
            ("source.zip", ARCHIVE),
            ("query.sql", DATABASE),
            ("app.sqlite3", DATABASE),
            ("run.sh", SHELL),
            ("run.ps1", SHELL),
            ("App.vue", VUE),
        ] {
            assert_eq!(for_path(Path::new(name)), expected, "{name}");
            assert_ne!(expected, DEFAULT, "{name}");
        }
    }

    #[test]
    fn matching_is_case_insensitive() {
        for name in [
            "MAIN.RS",
            "App.TSX",
            "Photo.JPEG",
            "CONFIG.YAML",
            "Dockerfile",
            "Cargo.TOML",
        ] {
            assert_eq!(
                for_path(Path::new(name)),
                for_path(Path::new(&name.to_ascii_lowercase()))
            );
        }
    }

    #[test]
    fn basenames_and_dotfiles_override_extensions() {
        for (name, expected) in [
            ("Cargo.toml", RUST),
            ("Cargo.lock", RUST),
            ("Dockerfile", DOCKER),
            ("Dockerfile.dev", DOCKER),
            ("Containerfile", DOCKER),
            (".gitignore", GIT),
            (".gitattributes", GIT),
            (".gitmodules", GIT),
            ("Makefile", CONFIG),
            (".env", CONFIG),
            (".env.production", CONFIG),
            (".editorconfig", CONFIG),
            (".zshrc", SHELL),
            ("tsconfig.json", TYPESCRIPT),
        ] {
            assert_eq!(
                for_path(&Path::new("workspace").join(name)),
                expected,
                "{name}"
            );
        }
    }

    #[test]
    fn compound_suffixes_do_not_match_parent_directories_or_substrings() {
        assert_eq!(for_path(Path::new("bundle.min.js.map")), JAVASCRIPT);
        assert_eq!(for_path(Path::new("styles.css.map")), CSS);
        assert_eq!(for_path(Path::new("workspace.rs/notes")), DEFAULT);
        assert_eq!(for_path(Path::new("foojs.map")), DEFAULT);
        assert_eq!(for_path(Path::new("Dockerfileish")), DEFAULT);
    }

    #[test]
    fn unknown_and_extensionless_files_have_a_safe_fallback() {
        for name in [
            "",
            "/",
            "LICENSE",
            ".unknown",
            "file.unknown",
            "notes.",
            "文档",
        ] {
            assert_eq!(for_path(Path::new(name)), DEFAULT, "{name}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn non_utf8_names_do_not_panic_or_hide_valid_extensions() {
        use std::ffi::OsStr;
        use std::os::unix::ffi::OsStrExt;
        assert_eq!(for_path(Path::new(OsStr::from_bytes(b"\xff.rs"))), RUST);
        assert_eq!(
            for_path(Path::new(OsStr::from_bytes(b"file.\xff"))),
            DEFAULT
        );
    }

    #[test]
    fn light_and_dark_palettes_preserve_color_and_fallback_contrast() {
        assert_eq!(JAVASCRIPT.color(false), 0xcbcb41);
        assert_eq!(JAVASCRIPT.color(true), 0xb7b73b);
        assert_eq!(PYTHON.color(true), 0x498ba7);
        assert_eq!(DEFAULT.color(false), 0xd4d7d6);
        assert_eq!(DEFAULT.color(true), 0x6d8086);
        for icon in [
            RUST, PYTHON, JAVASCRIPT, TYPESCRIPT, YAML, CONFIG, GIT, IMAGE, PDF, SHELL,
        ] {
            for light in [false, true] {
                let color = icon.color(light);
                assert!(color <= 0xffffff);
                assert_ne!((color >> 16) & 0xff, color & 0xff);
            }
        }
    }

    #[test]
    fn every_identifier_has_a_bundled_safe_svg() {
        let mut paths = std::collections::HashSet::new();
        for (path, svg) in ASSETS {
            assert!(paths.insert(path), "duplicate asset: {path}");
            assert!(svg.contains("<svg"), "{path}");
            assert!(svg.contains("viewBox=\"4 4 24 24\""), "{path}");
            assert!(svg.contains("<path"), "{path}");
            assert!(!svg.contains("<script") && !svg.contains("href="), "{path}");
        }
    }
}
