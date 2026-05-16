use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Violation {
    pub Path: PathBuf,
    pub Line: usize,
    pub FunctionName: String,
    pub SuggestedName: String,
}

pub fn CheckRustCasing(paths: &[PathBuf]) -> Result<Vec<Violation>, String> {
    let mut files = Vec::new();
    for path in paths {
        CollectRustFiles(path, &mut files)?;
    }

    files.sort();
    files.dedup();

    let mut violations = Vec::new();
    for path in files {
        let content = fs::read_to_string(&path)
            .map_err(|err| format!("failed to read '{}': {err}", path.display()))?;
        violations.extend(CheckRustSource(&path, &content));
    }

    Ok(violations)
}

pub fn CheckRustSource(path: &Path, source: &str) -> Vec<Violation> {
    let mut violations = Vec::new();
    let mut pending_test_attribute = false;
    let mut impl_stack: Vec<ImplScope> = Vec::new();

    for (index, line) in source.lines().enumerate() {
        let line_number = index + 1;
        let trimmed = line.trim();

        if trimmed.starts_with("#") {
            if trimmed.starts_with("#[test") {
                pending_test_attribute = true;
            }
            UpdateImplStackFromLine(trimmed, &mut impl_stack);
            continue;
        }

        let fn_name = ExtractFunctionName(trimmed);

        if let Some(name) = fn_name {
            let in_trait_impl = impl_stack.iter().any(|scope| scope.IsTraitImpl);
            if pending_test_attribute || in_trait_impl {
                pending_test_attribute = false;
                UpdateImplStackFromLine(trimmed, &mut impl_stack);
                continue;
            }

            if name == "main" {
                continue;
            }

            if !IsPascalCaseFunctionName(&name) {
                violations.push(Violation {
                    Path: path.to_path_buf(),
                    Line: line_number,
                    FunctionName: name.clone(),
                    SuggestedName: SuggestPascalCase(&name),
                });
            }
        }

        if !trimmed.is_empty() && !trimmed.starts_with("#") {
            pending_test_attribute = false;
        }

        UpdateImplStackFromLine(trimmed, &mut impl_stack);
    }

    violations
}

pub fn IsPascalCaseFunctionName(name: &str) -> bool {
    if name.is_empty() {
        return false;
    }

    let mut chars = name.chars();
    let first = chars.next().unwrap();
    if !first.is_ascii_uppercase() {
        return false;
    }

    if name.contains('_') {
        return false;
    }

    chars.all(|ch| ch.is_ascii_alphanumeric())
}

pub fn SuggestPascalCase(name: &str) -> String {
    if name.is_empty() {
        return String::new();
    }

    let mut output = String::new();
    for segment in name.split('_') {
        if segment.is_empty() {
            continue;
        }

        let mut chars = segment.chars();
        if let Some(first) = chars.next() {
            output.push(first.to_ascii_uppercase());
        }
        for ch in chars {
            output.push(ch.to_ascii_lowercase());
        }
    }

    if output.is_empty() {
        name.to_string()
    } else {
        output
    }
}

fn CollectRustFiles(path: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
    if ShouldSkipPath(path) {
        return Ok(());
    }

    if path.is_file() {
        if path.extension().and_then(|it| it.to_str()) == Some("rs") {
            files.push(path.to_path_buf());
        }
        return Ok(());
    }

    if !path.is_dir() {
        return Ok(());
    }

    let entries = fs::read_dir(path)
        .map_err(|err| format!("failed to read dir '{}': {err}", path.display()))?;

    for entry in entries {
        let entry =
            entry.map_err(|err| format!("failed to iterate dir '{}': {err}", path.display()))?;
        CollectRustFiles(&entry.path(), files)?;
    }

    Ok(())
}

fn ShouldSkipPath(path: &Path) -> bool {
    path.components().any(|component| {
        let value = component.as_os_str().to_string_lossy();
        value == "target"
            || value == ".git"
            || value == "vendor"
            || value == "third_party"
            || value == "third-party"
            || value == "Cargo.lock"
    })
}

fn ExtractFunctionName(line: &str) -> Option<String> {
    if line.starts_with("//") {
        return None;
    }

    let tokens: Vec<&str> = line.split_whitespace().collect();
    let fn_index = tokens.iter().position(|token| *token == "fn")?;
    if fn_index + 1 >= tokens.len() {
        return None;
    }

    let name_token = tokens[fn_index + 1];
    let open_paren = name_token.find('(')?;
    let name = &name_token[..open_paren];
    if name.is_empty() {
        return None;
    }

    if name
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
    {
        Some(name.to_string())
    } else {
        None
    }
}

#[derive(Debug)]
struct ImplScope {
    IsTraitImpl: bool,
    BraceDepth: usize,
}

fn UpdateImplStackFromLine(line: &str, impl_stack: &mut Vec<ImplScope>) {
    let trimmed = line.trim();
    if trimmed.starts_with("impl") {
        let is_trait_impl = trimmed.contains(" for ");
        let open_count = trimmed.chars().filter(|ch| *ch == '{').count();
        let close_count = trimmed.chars().filter(|ch| *ch == '}').count();
        if open_count > 0 {
            let initial_depth = open_count.saturating_sub(close_count);
            if initial_depth > 0 {
                impl_stack.push(ImplScope {
                    IsTraitImpl: is_trait_impl,
                    BraceDepth: initial_depth,
                });
                return;
            }
        }
    }

    let open_count = trimmed.chars().filter(|ch| *ch == '{').count();
    let close_count = trimmed.chars().filter(|ch| *ch == '}').count();

    if let Some(top) = impl_stack.last_mut() {
        top.BraceDepth += open_count;
        top.BraceDepth = top.BraceDepth.saturating_sub(close_count);
        if top.BraceDepth == 0 {
            impl_stack.pop();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn SuggestsPascalCaseFromSnakeCase() {
        assert_eq!(
            SuggestPascalCase("build_render_command_plan"),
            "BuildRenderCommandPlan"
        );
        assert_eq!(
            SuggestPascalCase("validate_wgpu_draw_inputs"),
            "ValidateWgpuDrawInputs"
        );
        assert_eq!(SuggestPascalCase("AlreadyPascal"), "Alreadypascal");
    }

    #[test]
    fn ValidatesPascalCaseNames() {
        assert!(IsPascalCaseFunctionName("BuildRenderCommandPlan"));
        assert!(!IsPascalCaseFunctionName("build_render_command_plan"));
        assert!(!IsPascalCaseFunctionName("buildRenderCommandPlan"));
        assert!(!IsPascalCaseFunctionName("Build_render_command_plan"));
        assert!(!IsPascalCaseFunctionName("_buildThing"));
    }

    #[test]
    fn DetectsAndSkipsExpectedDefinitions() {
        let source = r#"
            pub fn build_render_command_plan() {}
            pub fn BuildRenderCommandPlan() {}
            pub(crate) fn bad_name() {}
            fn bad_private_name() {}

            #[test]
            fn should_skip_test_name() {}

            impl Default for Foo {
                fn default() -> Self { Self {} }
            }

            impl fmt::Display for Foo {
                fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { write!(f, "") }
            }

            fn CallExternal(device: &Device) {
                device.create_shader_module(Default::default());
            }
        "#;

        let violations = CheckRustSource(Path::new("sample.rs"), source);
        let names: Vec<String> = violations.into_iter().map(|it| it.FunctionName).collect();

        assert!(names.contains(&"build_render_command_plan".to_string()));
        assert!(names.contains(&"bad_name".to_string()));
        assert!(names.contains(&"bad_private_name".to_string()));
        assert!(!names.contains(&"default".to_string()));
        assert!(!names.contains(&"fmt".to_string()));
        assert!(!names.contains(&"should_skip_test_name".to_string()));
    }

    #[test]
    fn CollectsOnlyRustFilesAndSkipsTargetLikeDirectories() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("wyrmfmt_test_{unique}"));
        fs::create_dir_all(root.join("src")).unwrap();
        fs::create_dir_all(root.join("target")).unwrap();

        fs::write(root.join("src/good.rs"), "pub fn BuildGood() {}\n").unwrap();
        fs::write(root.join("src/ignore.txt"), "ignored\n").unwrap();
        fs::write(root.join("target/bad.rs"), "pub fn bad_name() {}\n").unwrap();

        let violations = CheckRustCasing(&[root.clone()]).unwrap();
        assert!(violations.is_empty());

        fs::remove_dir_all(&root).unwrap();
    }
}
