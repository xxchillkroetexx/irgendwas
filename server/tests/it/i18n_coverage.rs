//! Static key-coverage and hard-coded-text check (i18n-foundation, rules C1 to C10).
//!
//! `scan` is a pure function over (path, source) pairs and the raw catalog texts, so each
//! rule is tested on synthetic input (AC17, AC18). The real-tree test (AC16) runs it over
//! `app/src`, `server/src` and `frontend/src` and fails on any finding.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use proc_macro2::{Delimiter, TokenStream, TokenTree};
use serde::de::{Deserialize, Deserializer, MapAccess, Visitor};

/// First segments of keys that exist (or will exist) in the catalogs. The constant makes C5
/// catch a typo even before a namespace has its first key.
const NAMESPACES: &[&str] = &[
    "app",
    "auth",
    "common",
    "email",
    "error",
    "exclusion",
    "flash",
    "footer",
    "group",
    "home",
    "language",
    "nav",
    "user",
    "validation",
    "wishlist",
];

/// Literals that look like keys but are not. Each entry needs a comment saying why.
const NOT_KEYS: &[&str] = &[];

/// Attribute names in `view!` whose string value is user-visible text.
const TEXT_ATTRIBUTES: &[&str] = &["aria-label", "title", "placeholder", "alt", "text", "label"];

#[derive(Debug, Clone, PartialEq, Eq)]
struct Finding {
    file: String,
    line: usize,
    rule: &'static str,
    detail: String,
}

impl fmt::Display for Finding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}:{} {} {}",
            self.file, self.line, self.rule, self.detail
        )
    }
}

// ------------------------------------------------------------------ catalogs

/// A JSON object of string to string, in file order, duplicates kept.
struct Entries(Vec<(String, String)>);

impl<'de> Deserialize<'de> for Entries {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = Entries;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("an object of string to string")
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Entries, A::Error> {
                let mut entries = Vec::new();
                while let Some((key, value)) = map.next_entry::<String, String>()? {
                    entries.push((key, value));
                }
                Ok(Entries(entries))
            }
        }
        deserializer.deserialize_map(V)
    }
}

type Catalog = BTreeMap<String, String>;

/// `{name}` placeholders of a message.
fn placeholders(message: &str) -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    let mut rest = message;
    while let Some(open) = rest.find('{') {
        let after = &rest[open + 1..];
        match after.find('}') {
            Some(close) => {
                let name = &after[..close];
                if !name.is_empty() && name.chars().all(|c| c.is_alphanumeric() || c == '_') {
                    found.insert(name.to_string());
                }
                rest = &after[close + 1..];
            }
            None => break,
        }
    }
    found
}

/// Placeholders of a key in the English catalog; plurals: the union of `.one` and `.other`.
/// `None` if the key is not in the catalog.
fn key_placeholders(en: &Catalog, key: &str) -> Option<BTreeSet<String>> {
    if let Some(message) = en.get(key) {
        return Some(placeholders(message));
    }
    let forms: Vec<&String> = ["one", "other"]
        .iter()
        .filter_map(|form| en.get(&format!("{key}.{form}")))
        .collect();
    if forms.is_empty() {
        return None;
    }
    Some(forms.into_iter().flat_map(|m| placeholders(m)).collect())
}

fn catalog_findings(file: &str, json: &str, findings: &mut Vec<Finding>) -> Catalog {
    let mut push = |rule: &'static str, detail: String| {
        findings.push(Finding {
            file: file.to_string(),
            line: 0,
            rule,
            detail,
        });
    };
    let entries = match serde_json::from_str::<Entries>(json) {
        Ok(entries) => entries.0,
        Err(err) => {
            push("C1", format!("not a JSON object of strings: {err}"));
            return Catalog::new();
        }
    };

    let mut seen = BTreeSet::new();
    for (key, value) in &entries {
        if !seen.insert(key.as_str()) {
            push("C1", format!("duplicate key {key}"));
        }
        if value.trim().is_empty() {
            push("C1", format!("empty value for key {key}"));
        }
        if !key.contains('.') {
            push("C1", format!("key without a dot: {key}"));
        }
    }

    let catalog: Catalog = entries.into_iter().collect();
    for key in catalog.keys() {
        if let Some(base) = key.strip_suffix(".one") {
            if !catalog.contains_key(&format!("{base}.other")) {
                push("C4", format!("{key} without {base}.other"));
            }
            if catalog.contains_key(base) {
                push("C4", format!("plain key {base} next to {base}.one/.other"));
            }
        }
        if let Some(base) = key.strip_suffix(".other") {
            if !catalog.contains_key(&format!("{base}.one")) {
                push("C4", format!("{key} without {base}.one"));
            }
            if catalog.contains_key(base) && !catalog.contains_key(&format!("{base}.one")) {
                push("C4", format!("plain key {base} next to {base}.other"));
            }
        }
    }
    catalog
}

// ----------------------------------------------------------- token utilities

fn is_punct(tt: &TokenTree, ch: char) -> bool {
    matches!(tt, TokenTree::Punct(p) if p.as_char() == ch)
}

fn is_ident(tt: &TokenTree, name: &str) -> bool {
    matches!(tt, TokenTree::Ident(i) if i == name)
}

/// The unescaped value of a string literal (`"…"`, `r"…"`, `r#"…"#`); `None` for every
/// other literal (byte strings, chars, numbers).
fn string_value(tt: &TokenTree) -> Option<String> {
    match tt {
        TokenTree::Literal(lit) => syn::parse_str::<syn::LitStr>(&lit.to_string())
            .ok()
            .map(|s| s.value()),
        _ => None,
    }
}

fn line_of(tt: &TokenTree) -> usize {
    tt.span().start().line
}

fn trees(stream: TokenStream) -> Vec<TokenTree> {
    stream.into_iter().collect()
}

/// Splits a macro's token list at top-level commas.
fn split_args(stream: TokenStream) -> Vec<Vec<TokenTree>> {
    let mut args = Vec::new();
    let mut current = Vec::new();
    for tt in stream {
        if is_punct(&tt, ',') {
            args.push(std::mem::take(&mut current));
        } else {
            current.push(tt);
        }
    }
    if !current.is_empty() {
        args.push(current);
    }
    args
}

/// `#[doc = …]` / `#![doc = …]`: the bracket group at `index` is a doc attribute.
fn is_doc_attribute(list: &[TokenTree], index: usize) -> bool {
    let TokenTree::Group(group) = &list[index] else {
        return false;
    };
    if group.delimiter() != Delimiter::Bracket {
        return false;
    }
    let hash_before = (index >= 1 && is_punct(&list[index - 1], '#'))
        || (index >= 2 && is_punct(&list[index - 1], '!') && is_punct(&list[index - 2], '#'));
    let inner = trees(group.stream());
    hash_before && inner.first().is_some_and(|tt| is_ident(tt, "doc"))
}

fn is_key_shaped(s: &str, namespaces: &BTreeSet<String>) -> bool {
    let mut segments = s.split('.');
    let first = segments.next().unwrap_or("");
    let mut chars = first.chars();
    let first_ok = chars.next().is_some_and(|c| c.is_ascii_lowercase())
        && chars.all(|c| c.is_ascii_alphanumeric());
    let rest: Vec<&str> = segments.collect();
    first_ok
        && !rest.is_empty()
        && rest.iter().all(|seg| {
            !seg.is_empty() && seg.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        })
        && namespaces.contains(first)
}

// ---------------------------------------------------------------------- scan

struct FileScan<'a> {
    file: &'a str,
    en: &'a Catalog,
    /// Every string literal outside doc attributes: (line, value).
    literals: Vec<(usize, String)>,
    findings: Vec<Finding>,
    allow_date_format: bool,
}

impl FileScan<'_> {
    fn finding(&mut self, line: usize, rule: &'static str, detail: String) {
        self.findings.push(Finding {
            file: self.file.to_string(),
            line,
            rule,
            detail,
        });
    }

    fn walk(&mut self, list: &[TokenTree]) {
        for (i, tt) in list.iter().enumerate() {
            match tt {
                TokenTree::Literal(_) => {
                    if let Some(value) = string_value(tt) {
                        self.literals.push((line_of(tt), value));
                    }
                }
                TokenTree::Group(group) => {
                    if is_doc_attribute(list, i) {
                        continue;
                    }
                    let inner = trees(group.stream());
                    let after_bang = i >= 2 && is_punct(&list[i - 1], '!');
                    let macro_name = after_bang.then(|| match &list[i - 2] {
                        TokenTree::Ident(name) => Some(name.to_string()),
                        _ => None,
                    });
                    match macro_name.flatten().as_deref() {
                        Some("t") => self.check_t_macro(group.stream(), false),
                        Some("t_plural") => self.check_t_macro(group.stream(), true),
                        Some("view") if group.delimiter() == Delimiter::Brace => {
                            self.check_view(&inner)
                        }
                        _ => {}
                    }
                    if group.delimiter() == Delimiter::Parenthesis && i >= 2 {
                        if is_punct(&list[i - 2], '.') && is_ident(&list[i - 1], "t") {
                            self.check_method_t(group.stream());
                        }
                        if !self.allow_date_format
                            && is_punct(&list[i - 2], '.')
                            && is_ident(&list[i - 1], "format")
                            && inner
                                .iter()
                                .filter_map(string_value)
                                .any(|s| s.contains('%'))
                        {
                            self.finding(
                                line_of(&list[i - 1]),
                                "C10",
                                "ad-hoc date formatting: .format(\"%…\"); use app::format".into(),
                            );
                        }
                    }
                    if !self.allow_date_format
                        && group.delimiter() == Delimiter::Parenthesis
                        && i >= 1
                        && is_ident(&list[i - 1], "format_local")
                    {
                        self.finding(
                            line_of(&list[i - 1]),
                            "C10",
                            "ad-hoc date formatting: format_local(…); use app::format".into(),
                        );
                    }
                    self.walk(&inner);
                }
                _ => {}
            }
        }
    }

    /// C8 for `t!(i18n, "key", a = …)` and `t_plural!(i18n, "base", n, a = …)`.
    fn check_t_macro(&mut self, stream: TokenStream, plural: bool) {
        let args = split_args(stream);
        let Some(key_arg) = args.get(1) else { return };
        let [key_tt] = key_arg.as_slice() else { return };
        let Some(key) = string_value(key_tt) else {
            return;
        };
        let Some(expected) = key_placeholders(self.en, &key) else {
            return; // C5 reports unknown keys
        };
        let first_named = 2;
        let given: BTreeSet<String> = args
            .iter()
            .skip(first_named)
            .filter_map(|arg| match arg.as_slice() {
                [TokenTree::Ident(name), eq, rest @ ..]
                    if is_punct(eq, '=') && !rest.first().is_some_and(|t| is_punct(t, '=')) =>
                {
                    Some(name.to_string())
                }
                _ => None,
            })
            .collect();
        if given != expected {
            let macro_name = if plural { "t_plural!" } else { "t!" };
            self.finding(
                line_of(key_tt),
                "C8",
                format!(
                    "{macro_name} arguments for {key} are {given:?}, the placeholders are {expected:?}"
                ),
            );
        }
    }

    /// C8 for `.t("key")`: the key must have no placeholders.
    fn check_method_t(&mut self, stream: TokenStream) {
        let args = split_args(stream);
        let Some([first]) = args.first().map(Vec::as_slice) else {
            return;
        };
        let Some(key) = string_value(first) else {
            return;
        };
        if let Some(expected) = key_placeholders(self.en, &key)
            && !expected.is_empty()
        {
            self.finding(
                line_of(first),
                "C8",
                format!(".t({key:?}) but {key} has placeholders {expected:?}; use t!"),
            );
        }
    }

    /// C9 for the literals directly inside one `view! { … }`.
    fn check_view(&mut self, list: &[TokenTree]) {
        for (j, tt) in list.iter().enumerate() {
            // `{"Login"}`: a block whose only token is a string literal is a text node in
            // disguise (a bypass of the text-node rule).
            if let TokenTree::Group(group) = tt
                && group.delimiter() == Delimiter::Brace
            {
                let inner = trees(group.stream());
                // Only a block in child position (right after a tag's `>`, or first): not an
                // attribute value and not the branch of an `if`/`else` expression.
                let is_child = j == 0 || is_punct(&list[j - 1], '>');
                if is_child
                    && let [only] = inner.as_slice()
                    && let Some(text) = string_value(only)
                    && text.chars().any(char::is_alphabetic)
                {
                    self.finding(
                        line_of(only),
                        "C9",
                        format!("hard-coded text in a block: {text:?}"),
                    );
                }
                continue;
            }
            let Some(text) = string_value(tt) else {
                continue;
            };
            if j >= 1 && is_punct(&list[j - 1], '=') {
                // The attribute name is `ident (("-" | ":") ident)*` right before the `=`.
                let mut name = String::new();
                let mut k = j - 1; // index of `=`
                while k >= 1 {
                    if let TokenTree::Ident(ident) = &list[k - 1] {
                        name.insert_str(0, &ident.to_string());
                        k -= 1;
                        if k >= 2
                            && (is_punct(&list[k - 1], '-') || is_punct(&list[k - 1], ':'))
                            && matches!(list[k - 2], TokenTree::Ident(_))
                        {
                            let TokenTree::Punct(sep) = &list[k - 1] else {
                                unreachable!()
                            };
                            name.insert(0, sep.as_char());
                            k -= 1;
                            continue;
                        }
                    }
                    break;
                }
                if TEXT_ATTRIBUTES.contains(&name.as_str()) && text.chars().any(char::is_alphabetic)
                {
                    self.finding(
                        line_of(tt),
                        "C9",
                        format!("hard-coded text in attribute {name}: {text:?}"),
                    );
                }
            } else if text.chars().any(char::is_alphabetic) {
                self.finding(line_of(tt), "C9", format!("hard-coded text node: {text:?}"));
            }
        }
    }
}

fn scan(files: &[(PathBuf, String)], en_json: &str, de_json: &str) -> Vec<Finding> {
    scan_with(files, en_json, de_json, NOT_KEYS)
}

fn scan_with(
    files: &[(PathBuf, String)],
    en_json: &str,
    de_json: &str,
    not_keys: &[&str],
) -> Vec<Finding> {
    let mut findings = Vec::new();
    let en = catalog_findings("locales/en.json", en_json, &mut findings);
    let de = catalog_findings("locales/de.json", de_json, &mut findings);

    // C2
    for key in en.keys().filter(|k| !de.contains_key(*k)) {
        findings.push(Finding {
            file: "locales/de.json".into(),
            line: 0,
            rule: "C2",
            detail: format!("key {key} is in en.json but not in de.json"),
        });
    }
    for key in de.keys().filter(|k| !en.contains_key(*k)) {
        findings.push(Finding {
            file: "locales/en.json".into(),
            line: 0,
            rule: "C2",
            detail: format!("key {key} is in de.json but not in en.json"),
        });
    }
    // C3
    for (key, en_message) in &en {
        if let Some(de_message) = de.get(key) {
            let (a, b) = (placeholders(en_message), placeholders(de_message));
            if a != b {
                findings.push(Finding {
                    file: "locales/de.json".into(),
                    line: 0,
                    rule: "C3",
                    detail: format!("key {key}: en placeholders {a:?}, de placeholders {b:?}"),
                });
            }
        }
    }

    let all_keys: BTreeSet<&String> = en.keys().chain(de.keys()).collect();
    let namespaces: BTreeSet<String> = NAMESPACES
        .iter()
        .map(|s| s.to_string())
        .chain(
            all_keys
                .iter()
                .filter_map(|k| k.split('.').next().map(str::to_string)),
        )
        .collect();

    let mut used_literals: BTreeSet<String> = BTreeSet::new();
    for (path, source) in files {
        let file = path.to_string_lossy().replace('\\', "/");
        let stream = match TokenStream::from_str(source) {
            Ok(stream) => stream,
            Err(err) => {
                findings.push(Finding {
                    file,
                    line: 0,
                    rule: "C0",
                    detail: format!("file does not tokenise: {err}"),
                });
                continue;
            }
        };
        let mut scan = FileScan {
            allow_date_format: file.ends_with("app/src/format.rs")
                || file.ends_with("app/src/tz.rs"),
            file: &file,
            en: &en,
            literals: Vec::new(),
            findings: Vec::new(),
        };
        scan.walk(&trees(stream));

        let literals = std::mem::take(&mut scan.literals);
        for (line, literal) in &literals {
            used_literals.insert(literal.clone());
            if not_keys.contains(&literal.as_str()) {
                continue;
            }
            // C6
            let namespace_prefix = literal
                .split_once('.')
                .map(|(first, _)| first)
                .is_some_and(|first| namespaces.contains(first));
            if namespace_prefix && (literal.contains('{') || literal.ends_with('.')) {
                scan.finding(*line, "C6", format!("dynamic key literal {literal:?}"));
                continue;
            }
            // C5
            if is_key_shaped(literal, &namespaces) {
                let exists = all_keys.contains(literal)
                    || (all_keys.contains(&format!("{literal}.one"))
                        && all_keys.contains(&format!("{literal}.other")));
                if !exists {
                    scan.finding(*line, "C5", format!("unknown key {literal}"));
                }
            }
        }
        findings.append(&mut scan.findings);
    }

    // C7
    for key in &all_keys {
        let base = key
            .strip_suffix(".one")
            .or_else(|| key.strip_suffix(".other"));
        let used = used_literals.contains(key.as_str())
            || base.is_some_and(|base| used_literals.contains(base));
        if !used {
            findings.push(Finding {
                file: if en.contains_key(*key) {
                    "locales/en.json"
                } else {
                    "locales/de.json"
                }
                .into(),
                line: 0,
                rule: "C7",
                detail: format!("catalog key {key} is not used by any code"),
            });
        }
    }
    findings
}

// ------------------------------------------------------------- real tree (AC16)

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root")
        .to_path_buf()
}

fn collect_sources(root: &Path, dir: &Path, out: &mut Vec<(PathBuf, String)>) {
    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("read {}: {e}", dir.display()))
        .map(|e| e.unwrap().path())
        .collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            collect_sources(root, &path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            let relative = path.strip_prefix(root).unwrap().to_path_buf();
            let name = relative.to_string_lossy().replace('\\', "/");
            if name == "app/src/test_support.rs" || name.starts_with("app/src/test_support/") {
                continue;
            }
            out.push((relative, std::fs::read_to_string(&path).unwrap()));
        }
    }
}

fn real_inputs() -> (Vec<(PathBuf, String)>, String, String) {
    let root = workspace_root();
    let mut files = Vec::new();
    for dir in ["app/src", "server/src", "frontend/src"] {
        collect_sources(&root, &root.join(dir), &mut files);
    }
    assert!(files.len() > 10, "found only {} source files", files.len());
    let en = std::fs::read_to_string(root.join("locales/en.json")).unwrap();
    let de = std::fs::read_to_string(root.join("locales/de.json")).unwrap();
    (files, en, de)
}

fn report(findings: &[Finding]) -> String {
    findings
        .iter()
        .map(Finding::to_string)
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn ac16_real_tree_has_no_findings() {
    let (files, en, de) = real_inputs();
    let findings = scan(&files, &en, &de);
    assert!(
        findings.is_empty(),
        "{} i18n finding(s):\n{}",
        findings.len(),
        report(&findings)
    );
}

#[test]
fn ac16_catalogs_have_identical_key_sets() {
    let (_, en, de) = real_inputs();
    let mut findings = Vec::new();
    let en_catalog = catalog_findings("locales/en.json", &en, &mut findings);
    let de_catalog = catalog_findings("locales/de.json", &de, &mut findings);
    let only_en: Vec<_> = en_catalog
        .keys()
        .filter(|k| !de_catalog.contains_key(*k))
        .collect();
    let only_de: Vec<_> = de_catalog
        .keys()
        .filter(|k| !en_catalog.contains_key(*k))
        .collect();
    assert!(
        only_en.is_empty() && only_de.is_empty(),
        "only in en.json: {only_en:?}\nonly in de.json: {only_de:?}"
    );
}

#[test]
fn ac16_every_catalog_key_is_used_by_non_test_code() {
    let (files, en, de) = real_inputs();
    let unused: Vec<Finding> = scan(&files, &en, &de)
        .into_iter()
        .filter(|f| f.rule == "C7")
        .collect();
    assert!(unused.is_empty(), "unused keys:\n{}", report(&unused));
}

#[test]
fn ac16_an_unused_key_in_both_catalogs_is_reported() {
    let (files, en, de) = real_inputs();
    let add = |json: &str| json.replacen('{', "{\n    \"nav.unusedProbe\": \"x\",", 1);
    let findings = scan(&files, &add(&en), &add(&de));
    let hit = findings
        .iter()
        .find(|f| f.rule == "C7" && f.detail.contains("nav.unusedProbe"))
        .unwrap_or_else(|| panic!("no C7 for the probe key:\n{}", report(&findings)));
    assert!(hit.to_string().contains("locales/en.json"), "{hit}");
}

#[test]
fn ac16_an_unknown_key_in_code_is_reported_with_file_and_line() {
    let (mut files, en, de) = real_inputs();
    files.push((
        PathBuf::from("server/src/probe.rs"),
        "fn probe(i18n: app::i18n::I18n) {\n    let _ = app::t!(i18n, \"nav.nope\");\n}\n".into(),
    ));
    let findings = scan(&files, &en, &de);
    let hit = findings
        .iter()
        .find(|f| f.rule == "C5" && f.detail.contains("nav.nope"))
        .unwrap_or_else(|| panic!("no C5 for nav.nope:\n{}", report(&findings)));
    assert_eq!(hit.file, "server/src/probe.rs");
    assert_eq!(hit.line, 2);
    assert!(
        hit.to_string().starts_with("server/src/probe.rs:2 C5 "),
        "{hit}"
    );
}

// -------------------------------------------------- synthetic input (AC17, AC18)

fn run(files: &[(&str, &str)], en: &str, de: &str) -> Vec<Finding> {
    let files: Vec<(PathBuf, String)> = files
        .iter()
        .map(|(p, s)| (PathBuf::from(p), s.to_string()))
        .collect();
    scan(&files, en, de)
}

/// A finding of `rule` whose file contains `file` and whose detail contains `detail`.
fn has(findings: &[Finding], rule: &str, file: &str, detail: &str) -> bool {
    findings
        .iter()
        .any(|f| f.rule == rule && f.file.contains(file) && f.detail.contains(detail))
}

fn assert_has(findings: &[Finding], rule: &str, file: &str, detail: &str) {
    assert!(
        has(findings, rule, file, detail),
        "expected {rule} in {file} mentioning {detail:?}, got:\n{}",
        report(findings)
    );
}

/// Catalogs and a source file that use every key exactly right: no findings.
const OK_EN: &str = r#"{"t.max": "Max {max}", "t.plain": "Plain",
    "t.items.one": "{count} item", "t.items.other": "{count} items"}"#;
const OK_DE: &str = r#"{"t.max": "Höchstens {max}", "t.plain": "Schlicht",
    "t.items.one": "{count} Artikel", "t.items.other": "{count} Artikel"}"#;
const OK_CODE: &str = r#"
fn ok(i18n: I18n, n: i64) {
    let _ = t!(i18n, "t.max", max = 1);
    let _ = i18n.t("t.plain");
    let _ = t_plural!(i18n, "t.items", n, count = n);
}
"#;

#[test]
fn baseline_synthetic_input_is_clean() {
    let findings = run(&[("app/src/ok.rs", OK_CODE)], OK_EN, OK_DE);
    assert!(findings.is_empty(), "{}", report(&findings));
}

#[test]
fn ac17_c1_catalog_shape() {
    let en = r#"{"t.dup": "a", "t.dup": "b", "t.empty": "  ", "nodot": "x"}"#;
    let findings = run(&[], en, "{}");
    assert_has(&findings, "C1", "en.json", "t.dup");
    assert_has(&findings, "C1", "en.json", "t.empty");
    assert_has(&findings, "C1", "en.json", "nodot");
    // not an object of strings at all
    let findings = run(&[], r#"{"t.a": 1}"#, "{}");
    assert_has(&findings, "C1", "en.json", "");
    let findings = run(&[], "{}", "[]");
    assert_has(&findings, "C1", "de.json", "");
}

#[test]
fn ac17_c2_key_sets_differ() {
    let findings = run(&[], r#"{"t.a": "x"}"#, r#"{"t.a": "x", "t.only_de": "y"}"#);
    assert_has(&findings, "C2", "en.json", "t.only_de");
    let findings = run(&[], r#"{"t.a": "x", "t.only_en": "y"}"#, r#"{"t.a": "x"}"#);
    assert_has(&findings, "C2", "de.json", "t.only_en");
}

#[test]
fn ac17_c3_placeholder_parity() {
    let findings = run(
        &[],
        r#"{"t.hi": "Hello {name}"}"#,
        r#"{"t.hi": "Hallo {user}"}"#,
    );
    assert_has(&findings, "C3", "de.json", "t.hi");
}

#[test]
fn ac17_c4_plural_shape() {
    let findings = run(&[], r#"{"t.x.one": "a"}"#, r#"{"t.x.one": "a"}"#);
    assert_has(&findings, "C4", "en.json", "t.x.one");
    assert_has(&findings, "C4", "de.json", "t.x.one");

    let findings = run(&[], r#"{"t.y.other": "a"}"#, r#"{"t.y.other": "a"}"#);
    assert_has(&findings, "C4", "en.json", "t.y.other");

    let both = r#"{"t.z": "plain", "t.z.one": "a", "t.z.other": "b"}"#;
    let findings = run(&[], both, both);
    assert_has(&findings, "C4", "en.json", "t.z");
}

#[test]
fn ac17_c5_used_keys_must_exist() {
    let findings = run(
        &[(
            "app/src/pages/x.rs",
            "fn f(i18n: I18n) { i18n.t(\"home.nope\"); }",
        )],
        "{}",
        "{}",
    );
    assert_has(&findings, "C5", "app/src/pages/x.rs", "home.nope");
    // t! and t_plural! are checked too, and a plural base needs both forms
    let findings = run(
        &[(
            "app/src/x.rs",
            "fn f() { t!(i18n, \"group.nope\"); t_plural!(i18n, \"group.few\", 2); }",
        )],
        r#"{"group.few.one": "a"}"#,
        r#"{"group.few.one": "a"}"#,
    );
    assert_has(&findings, "C5", "app/src/x.rs", "group.nope");
    assert_has(&findings, "C5", "app/src/x.rs", "group.few");
}

#[test]
fn ac17_c6_dynamic_keys() {
    let findings = run(
        &[(
            "app/src/x.rs",
            "fn f(s: &str) { let _ = format!(\"group.status.{}\", s); let _ = \"group.status.\"; }",
        )],
        "{}",
        "{}",
    );
    assert_has(&findings, "C6", "app/src/x.rs", "group.status.{}");
    assert_has(&findings, "C6", "app/src/x.rs", "group.status.");
}

#[test]
fn ac17_c7_unused_catalog_keys() {
    let findings = run(
        &[("app/src/ok.rs", OK_CODE)],
        r#"{"t.max": "Max {max}", "t.plain": "P", "t.never": "x",
            "t.items.one": "{count} item", "t.items.other": "{count} items"}"#,
        r#"{"t.max": "Höchstens {max}", "t.plain": "P", "t.never": "y",
            "t.items.one": "{count} Artikel", "t.items.other": "{count} Artikel"}"#,
    );
    assert_has(&findings, "C7", "en.json", "t.never");
    assert!(
        !has(&findings, "C7", "", "t.items") && !has(&findings, "C7", "", "t.max"),
        "used keys must not be reported:\n{}",
        report(&findings)
    );
}

#[test]
fn ac17_c8_arguments_must_match_placeholders() {
    // missing argument
    let findings = run(
        &[("app/src/a.rs", "fn f() { t!(i18n, \"t.max\"); }")],
        OK_EN,
        OK_DE,
    );
    assert_has(&findings, "C8", "app/src/a.rs", "t.max");
    // surplus argument
    let findings = run(
        &[(
            "app/src/b.rs",
            "fn f() { t!(i18n, \"t.max\", max = 1, foo = 2); }",
        )],
        OK_EN,
        OK_DE,
    );
    assert_has(&findings, "C8", "app/src/b.rs", "t.max");
    // `.t` on a key with placeholders
    let findings = run(
        &[("app/src/c.rs", "fn f() { i18n.t(\"t.max\"); }")],
        OK_EN,
        OK_DE,
    );
    assert_has(&findings, "C8", "app/src/c.rs", "t.max");
    // plurals: the union of .one and .other
    let findings = run(
        &[(
            "app/src/d.rs",
            "fn f(n: i64) { t_plural!(i18n, \"t.items\", n); }",
        )],
        OK_EN,
        OK_DE,
    );
    assert_has(&findings, "C8", "app/src/d.rs", "t.items");
    // wrong name
    let findings = run(
        &[(
            "app/src/e.rs",
            "fn f() { t!(i18n, \"t.max\", maximum = 1); }",
        )],
        OK_EN,
        OK_DE,
    );
    assert_has(&findings, "C8", "app/src/e.rs", "t.max");
}

#[test]
fn ac17_c9_hard_coded_text_in_view() {
    let findings = run(
        &[(
            "app/src/pages/a.rs",
            "fn f() { view! { <p>\"Hello\"</p> } }",
        )],
        "{}",
        "{}",
    );
    assert_has(&findings, "C9", "app/src/pages/a.rs", "Hello");

    let findings = run(
        &[(
            "app/src/pages/b.rs",
            "fn f() { view! { <input placeholder=\"Search\"/> } }",
        )],
        "{}",
        "{}",
    );
    assert_has(&findings, "C9", "app/src/pages/b.rs", "Search");

    // aria-label (name rebuilt from ident, `-`, ident) and a nested view! in a block
    let findings = run(
        &[(
            "app/src/pages/c.rs",
            "fn f() { view! { <div>{view! { <button aria-label=\"Close it\"></button> }}</div> } }",
        )],
        "{}",
        "{}",
    );
    assert_has(&findings, "C9", "app/src/pages/c.rs", "Close it");
}

#[test]
fn ac17_c10_ad_hoc_date_formatting() {
    let findings = run(
        &[(
            "app/src/groups.rs",
            "fn f(dt: D) { let _ = dt.format(\"%d.%m.%Y\"); }",
        )],
        "{}",
        "{}",
    );
    assert_has(&findings, "C10", "app/src/groups.rs", "format");

    let findings = run(
        &[(
            "app/src/groups.rs",
            "fn f(dt: D, tz: T) { let _ = format_local(dt, tz, \"%Y\"); }",
        )],
        "{}",
        "{}",
    );
    assert_has(&findings, "C10", "app/src/groups.rs", "format_local");
}

#[test]
fn ac17_findings_carry_line_numbers() {
    let findings = run(
        &[(
            "app/src/pages/a.rs",
            "fn f() {}\n\nfn g() {\n    view! { <p>\"Hello\"</p> }\n}\n",
        )],
        "{}",
        "{}",
    );
    let hit = findings.iter().find(|f| f.rule == "C9").expect("C9");
    assert_eq!(hit.line, 4);
    assert!(
        hit.to_string().starts_with("app/src/pages/a.rs:4 C9 "),
        "{hit}"
    );
}

#[test]
fn ac17_a_file_that_does_not_tokenise_is_a_finding() {
    let findings = run(
        &[("app/src/broken.rs", "fn f() { \"unterminated }")],
        "{}",
        "{}",
    );
    assert!(
        findings.iter().any(|f| f.file == "app/src/broken.rs"),
        "{}",
        report(&findings)
    );
}

#[test]
fn ac18_comments_and_doc_comments_are_ignored() {
    let code = r#"
// i18n.t("home.nope") in a line comment
/* i18n.t("home.nope") in a block comment */
/// Doc comment: t!(i18n, "home.nope") and "t.documented"
fn f() {
    //! inner doc "home.nope"
}
#[doc = "attribute doc with home.nope"]
fn g() {}
"#;
    let findings = run(&[("app/src/x.rs", code)], "{}", "{}");
    assert!(findings.is_empty(), "{}", report(&findings));

    // ...and they do not count as usage for C7.
    let findings = run(
        &[("app/src/x.rs", code)],
        r#"{"t.documented": "x"}"#,
        r#"{"t.documented": "y"}"#,
    );
    assert_has(&findings, "C7", "en.json", "t.documented");
}

#[test]
fn ac18_not_keys_are_exempt() {
    let files = vec![(
        PathBuf::from("app/src/x.rs"),
        "fn f() { let _ = \"home.example.com\"; let _ = \"group.notes.\"; }".to_string(),
    )];
    // Without the exemption both literals are findings...
    let findings = scan_with(&files, "{}", "{}", &[]);
    assert_has(&findings, "C5", "app/src/x.rs", "home.example.com");
    assert_has(&findings, "C6", "app/src/x.rs", "group.notes.");
    // ...with it they are not.
    let findings = scan_with(&files, "{}", "{}", &["home.example.com", "group.notes."]);
    assert!(findings.is_empty(), "{}", report(&findings));
}

#[test]
fn ac18_allowed_view_content_passes() {
    let code = r#"
fn f(x: bool) {
    view! { <p>"© "</p> }
    view! { <div class="mb-3" role="alert"/> }
    view! { <p>{if x { "yes" } else { "no" }}</p> }
    view! { <p>{"·"}</p> }
    view! { <p class={"btn"}>{if x { "yes" } else { "no" }}</p> }
}
"#;
    let findings = run(&[("app/src/pages/ok.rs", code)], "{}", "{}");
    assert!(findings.is_empty(), "{}", report(&findings));
}

#[test]
fn ac17_c9_block_with_only_a_text_literal_is_a_finding() {
    let findings = run(
        &[(
            "app/src/pages/a.rs",
            "fn f() { view! { <a href=\"/auth/login\" class=\"btn\">{\"Login\"}</a> } }",
        )],
        "{}",
        "{}",
    );
    assert_has(&findings, "C9", "app/src/pages/a.rs", "Login");
}

#[test]
fn ac18_plural_forms_satisfy_keys_and_arguments() {
    let code = "fn f(n: i64) { let _ = t_plural!(i18n, \"t.items\", n, count = n); }";
    let findings = run(
        &[("app/src/x.rs", code)],
        r#"{"t.items.one": "{count} item", "t.items.other": "{count} items"}"#,
        r#"{"t.items.one": "{count} Artikel", "t.items.other": "{count} Artikel"}"#,
    );
    assert!(findings.is_empty(), "{}", report(&findings));
}

#[test]
fn ac18_chrono_formatting_is_allowed_in_format_rs_and_tz_rs() {
    let code = "fn f(dt: D, tz: T) { let _ = dt.format(\"%d.%m.%Y\"); let _ = format_local(dt, tz, \"%Y\"); }";
    let findings = run(
        &[("app/src/format.rs", code), ("app/src/tz.rs", code)],
        "{}",
        "{}",
    );
    assert!(findings.is_empty(), "{}", report(&findings));
}

#[test]
fn ac18_strings_that_are_not_keys_are_ignored() {
    // Not key-shaped, or not in a namespace: no C5/C6.
    let code = r##"fn f() {
        let _ = "locale"; let _ = "Europe/Berlin"; let _ = "styles.css";
        let _ = "https://cdn.example.com/x.js"; let _ = b"home.nope"; let _ = 'x';
        let _ = r#"raw "quoted" text"#;
    }"##;
    let findings = run(&[("server/src/x.rs", code)], "{}", "{}");
    assert!(findings.is_empty(), "{}", report(&findings));
}
