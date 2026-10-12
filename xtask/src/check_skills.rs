//! `cargo xtask check-skills`: checks every skill in `.agents/skills/` against the Agent
//! Skills specification (<https://agentskills.io/specification>).

use std::fs;
use std::path::{Path, PathBuf};

use clap::Args;

/// Name length limit, from the spec's `name` field.
const MAX_NAME_CHARS: usize = 64;
/// Description length limit, from the spec's `description` field.
const MAX_DESCRIPTION_CHARS: usize = 1024;
/// The body must stay under this many lines, from the spec's "Progressive disclosure" section.
const MAX_BODY_LINES: usize = 500;

#[derive(Debug, Args)]
pub struct CheckSkillsArgs {
    /// Directory that holds one folder per skill
    #[arg(long, default_value = ".agents/skills")]
    pub dir: PathBuf,
}

/// Why the check could not run at all. A skill that breaks a rule is reported, not an error.
#[derive(Debug, thiserror::Error)]
pub enum CheckSkillsError {
    #[error("cannot read {path}: {source}")]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("no skill (a folder with a SKILL.md) found in {0}")]
    NoSkills(PathBuf),
}

/// Checks every skill under `args.dir`, prints one line per skill (or per problem), and
/// returns whether all of them passed.
pub fn run(args: &CheckSkillsArgs) -> Result<bool, CheckSkillsError> {
    let read_err = |path: &Path| {
        let path = path.to_path_buf();
        move |source| CheckSkillsError::Read { path, source }
    };
    let mut skills = Vec::new();
    for entry in fs::read_dir(&args.dir).map_err(read_err(&args.dir))? {
        let path = entry.map_err(read_err(&args.dir))?.path();
        if path.join("SKILL.md").is_file() {
            skills.push(path);
        }
    }
    skills.sort();
    if skills.is_empty() {
        return Err(CheckSkillsError::NoSkills(args.dir.clone()));
    }

    let mut all_ok = true;
    for dir in &skills {
        let dir_name = dir
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        let skill_md = dir.join("SKILL.md");
        let content = fs::read_to_string(&skill_md).map_err(read_err(&skill_md))?;
        let problems = check_skill(
            &dir_name,
            &content,
            dir.join("agents/openai.yaml").is_file(),
            |target| dir.join(target).exists(),
        );
        if problems.is_empty() {
            println!("ok {dir_name}");
        } else {
            all_ok = false;
            for problem in problems {
                println!("FAIL {dir_name}: {problem}");
            }
        }
    }
    Ok(all_ok)
}

/// Returns every rule the skill breaks. `link_exists` resolves a relative link target
/// against the skill's directory.
fn check_skill(
    dir_name: &str,
    content: &str,
    has_openai_yaml: bool,
    link_exists: impl Fn(&str) -> bool,
) -> Vec<String> {
    let mut problems = Vec::new();
    match split_frontmatter(content) {
        None => problems.push("SKILL.md has no frontmatter (--- block at the top)".to_owned()),
        Some((frontmatter, body)) => {
            let fields = parse_frontmatter(frontmatter);
            let field = |key: &str| {
                fields
                    .iter()
                    .find(|(name, _)| name == key)
                    .map(|(_, value)| value.as_str())
            };
            match field("name") {
                None | Some("") => problems.push("frontmatter has no `name`".to_owned()),
                Some(name) => problems.extend(check_name(name, dir_name)),
            }
            match field("description") {
                None | Some("") => problems.push("frontmatter has no `description`".to_owned()),
                Some(description) => problems.extend(check_description(description)),
            }
            let body_lines = body.lines().count();
            if body_lines >= MAX_BODY_LINES {
                problems.push(format!(
                    "body is {body_lines} lines, must be under {MAX_BODY_LINES}"
                ));
            }
            for target in relative_links(body) {
                if !link_exists(&target) {
                    problems.push(format!("broken link: {target}"));
                }
            }
        }
    }
    if !has_openai_yaml {
        problems.push("agents/openai.yaml is missing".to_owned());
    }
    problems
}

fn check_name(name: &str, dir_name: &str) -> Vec<String> {
    let mut problems = Vec::new();
    let len = name.chars().count();
    if len > MAX_NAME_CHARS {
        problems.push(format!(
            "name is {len} characters, must be at most {MAX_NAME_CHARS}"
        ));
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        problems.push(format!(
            "name `{name}` must use lowercase letters, digits, and hyphens only"
        ));
    }
    if name.starts_with('-') || name.ends_with('-') {
        problems.push(format!("name `{name}` must not start or end with a hyphen"));
    }
    if name.contains("--") {
        problems.push(format!("name `{name}` must not contain `--`"));
    }
    if name != dir_name {
        problems.push(format!(
            "name `{name}` must equal the directory name `{dir_name}`"
        ));
    }
    problems
}

fn check_description(description: &str) -> Option<String> {
    let len = description.chars().count();
    (len > MAX_DESCRIPTION_CHARS).then(|| {
        format!("description is {len} characters, must be at most {MAX_DESCRIPTION_CHARS}")
    })
}

/// Splits `---` delimited frontmatter from the body. `None` when the file does not start
/// with a closed frontmatter block.
fn split_frontmatter(content: &str) -> Option<(&str, &str)> {
    let rest = content.strip_prefix('\u{feff}').unwrap_or(content);
    let rest = rest
        .strip_prefix("---\r\n")
        .or_else(|| rest.strip_prefix("---\n"))?;
    let mut offset = 0;
    for line in rest.split_inclusive('\n') {
        if line.trim_end() == "---" {
            return Some((&rest[..offset], &rest[offset + line.len()..]));
        }
        offset += line.len();
    }
    None
}

/// Reads top-level `key: value` pairs. An indented line continues the previous value, which
/// covers folded (`>`) and literal (`|`) block scalars. Surrounding quotes are removed.
fn parse_frontmatter(frontmatter: &str) -> Vec<(String, String)> {
    let mut fields: Vec<(String, String)> = Vec::new();
    for line in frontmatter.lines() {
        if line.starts_with([' ', '\t']) {
            if let Some((_, value)) = fields.last_mut() {
                let text = line.trim();
                if !text.is_empty() {
                    if !value.is_empty() {
                        value.push(' ');
                    }
                    value.push_str(text);
                }
            }
        } else if let Some((key, value)) = line.split_once(':') {
            let value = value.trim();
            let value = if matches!(value, ">" | "|" | ">-" | "|-" | ">+" | "|+") {
                ""
            } else {
                unquote(value)
            };
            fields.push((key.trim().to_owned(), value.to_owned()));
        }
    }
    fields
}

fn unquote(value: &str) -> &str {
    for quote in ['"', '\''] {
        if let Some(inner) = value
            .strip_prefix(quote)
            .and_then(|rest| rest.strip_suffix(quote))
        {
            return inner;
        }
    }
    value
}

/// Relative link targets of the markdown `[text](target)` links in `body`, outside fenced
/// code blocks. External links (`scheme://`, `mailto:`) and anchor-only links are skipped,
/// and a `#fragment` is stripped.
fn relative_links(body: &str) -> Vec<String> {
    let mut targets = Vec::new();
    let mut in_fence = false;
    for line in body.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            continue;
        }
        let mut rest = line;
        while let Some(start) = rest.find("](") {
            rest = &rest[start + 2..];
            let Some(end) = rest.find(')') else { break };
            let raw = rest[..end].trim().trim_start_matches('<');
            // `[text](path "title")`: the target ends at the first space.
            let raw = raw.split([' ', '>']).next().unwrap_or_default();
            rest = &rest[end + 1..];
            if raw.contains("://") || raw.starts_with("mailto:") {
                continue;
            }
            let path = raw.split('#').next().unwrap_or_default();
            if !path.is_empty() {
                targets.push(path.to_owned());
            }
        }
    }
    targets
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOOD: &str = "---\nname: flow-demo\ndescription: Does a thing.\n---\n\n# Demo\n";

    fn check(content: &str) -> Vec<String> {
        check_skill("flow-demo", content, true, |_| true)
    }

    #[test]
    fn a_valid_skill_has_no_problems() {
        assert_eq!(check(GOOD), Vec::<String>::new());
    }

    #[test]
    fn missing_frontmatter_or_fields_fail() {
        assert!(check("# no frontmatter\n")[0].contains("no frontmatter"));
        assert!(check("---\nname: flow-demo\n---\nbody\n")[0].contains("no `description`"));
        assert!(check("---\ndescription: x\n---\nbody\n")[0].contains("no `name`"));
        assert!(
            check("---\nname: flow-demo\ndescription: x\nbody\n")[0].contains("no frontmatter")
        );
    }

    #[test]
    fn name_rules() {
        assert!(check_name("flow-demo-2", "flow-demo-2").is_empty());
        for bad in ["Flow", "flow_demo", "-flow", "flow-", "flow--demo", "flöw"] {
            assert!(!check_name(bad, bad).is_empty(), "{bad}");
        }
        assert!(!check_name(&"a".repeat(65), &"a".repeat(65)).is_empty());
        assert!(check_name(&"a".repeat(64), &"a".repeat(64)).is_empty());
        let mismatch = check_name("flow-a", "flow-b");
        assert_eq!(mismatch.len(), 1);
        assert!(mismatch[0].contains("directory name"));
    }

    #[test]
    fn description_rules() {
        assert!(check_description(&"d".repeat(1024)).is_none());
        assert!(check_description(&"d".repeat(1025)).is_some());
        let long = format!(
            "---\nname: flow-demo\ndescription: {}\n---\nbody\n",
            "d".repeat(1025)
        );
        assert!(check(&long)[0].contains("1025 characters"));
    }

    #[test]
    fn block_scalar_and_quoted_values_parse() {
        let content = "---\nname: \"flow-demo\"\ndescription: >\n  first\n  second\n---\nbody\n";
        let (frontmatter, _) = split_frontmatter(content).unwrap();
        let fields = parse_frontmatter(frontmatter);
        assert_eq!(fields[0], ("name".to_owned(), "flow-demo".to_owned()));
        assert_eq!(
            fields[1],
            ("description".to_owned(), "first second".to_owned())
        );
        assert!(check(content).is_empty());
    }

    #[test]
    fn body_must_be_under_500_lines() {
        let body = |lines: usize| {
            format!(
                "---\nname: flow-demo\ndescription: x\n---\n{}",
                "l\n".repeat(lines)
            )
        };
        assert!(check(&body(499)).is_empty());
        assert!(check(&body(500))[0].contains("500 lines"));
    }

    #[test]
    fn openai_yaml_must_exist() {
        let problems = check_skill("flow-demo", GOOD, false, |_| true);
        assert_eq!(problems, ["agents/openai.yaml is missing"]);
    }

    #[test]
    fn relative_links_skip_external_anchor_and_code() {
        let body = "[a](x.md) [b](https://e.com/p) [c](mailto:a@b.c) [d](#top) \
                    [e](../y.md#sec) [f](dir/ \"title\")\n```\n[g](nope.md)\n```\n";
        assert_eq!(relative_links(body), ["x.md", "../y.md", "dir/"]);
    }

    #[test]
    fn broken_links_fail() {
        let content = format!("{GOOD}\nSee [spec](missing.md) and [ok](here.md).\n");
        let problems = check_skill("flow-demo", &content, true, |t| t == "here.md");
        assert_eq!(problems, ["broken link: missing.md"]);
    }

    #[test]
    fn run_reports_each_skill_on_disk() {
        let root = std::env::temp_dir().join(format!("check-skills-{}", std::process::id()));
        let good = root.join("flow-demo");
        let bad = root.join("flow-bad");
        fs::create_dir_all(good.join("agents")).unwrap();
        fs::create_dir_all(&bad).unwrap();
        fs::write(good.join("SKILL.md"), GOOD).unwrap();
        fs::write(good.join("agents/openai.yaml"), "").unwrap();
        fs::write(bad.join("SKILL.md"), GOOD).unwrap();
        let args = CheckSkillsArgs { dir: root.clone() };
        let all_ok = run(&args).unwrap();
        fs::remove_dir_all(&root).unwrap();
        assert!(!all_ok, "flow-bad has a mismatched name and no openai.yaml");
    }
}
