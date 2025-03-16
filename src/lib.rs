use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

pub fn codegen(
    out_path: &Path,
    asset_dirs: &[PathBuf],
    extra_files: &[PathBuf],
) -> std::io::Result<()> {
    let mut output = String::new();
    let mut static_files = Vec::new();
    let mut module_map: HashMap<String, Vec<String>> = HashMap::new();

    output.push_str(
        r#"#[derive(Debug)]
pub struct StaticFile {
    pub file_name: &'static str,
    pub name: &'static str,
    pub mime: &'static str,
}
"#,
    );

    for asset_dir in asset_dirs {
        process_directory(
            asset_dir,
            asset_dir,
            &mut output,
            &mut static_files,
            &mut module_map,
            0,
        )?;
    }

    for file_path in extra_files {
        if let Some(parent) = file_path.parent() {
            process_file(
                file_path,
                parent,
                &mut output,
                &mut static_files,
                &mut module_map,
                0,
            )?;
        } else {
            process_file(
                file_path,
                Path::new(""),
                &mut output,
                &mut static_files,
                &mut module_map,
                0,
            )?;
        }
    }

    output.push_str(
        r#"
#[allow(dead_code)]
impl StaticFile {
    /// Get a single `StaticFile` by name, if it exists.
    #[must_use]
    pub fn get(name: &str) -> Option<&'static Self> {
        if let Some(pos) = STATICS.iter().position(|&s| name == s.name) {
            Some(STATICS[pos])
        } else {
            None
        }
    }
}

impl std::fmt::Display for StaticFile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name)
    }
}
"#,
    );

    let statics_array = static_files
        .iter()
        .map(|item| format!("\n    {}", item))
        .collect::<Vec<_>>()
        .join(",");

    output.push_str(&format!(
        "\nstatic STATICS: &[&StaticFile] = &[{}\n];\n",
        statics_array
    ));

    let mut out_file = File::create(out_path)?;
    out_file.write_all(output.as_bytes())?;

    Ok(())
}

fn process_directory(
    dir: &Path,
    base_dir: &Path,
    output: &mut String,
    static_files: &mut Vec<String>,
    module_map: &mut HashMap<String, Vec<String>>,
    indent_level: usize,
) -> std::io::Result<()> {
    let rel_path = dir.strip_prefix(base_dir).unwrap_or(dir);
    let dir_module_path = get_module_path(rel_path);

    let create_module = !rel_path.as_os_str().is_empty();

    if create_module {
        let module_name = rel_path
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .replace(['-', '.'], "_");

        let indent = "    ".repeat(indent_level);
        output.push_str(&format!("\n{}pub mod {} {{\n", indent, module_name));
        output.push_str(&format!("{}    use super::StaticFile;\n", indent));

        if !module_map.contains_key(&dir_module_path) {
            module_map.insert(dir_module_path.clone(), Vec::new());
        }
    }

    let next_indent = if create_module {
        indent_level + 1
    } else {
        indent_level
    };

    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_file() {
            process_file(
                &path,
                base_dir,
                output,
                static_files,
                module_map,
                next_indent,
            )?;
        }
    }

    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_dir() {
            process_directory(
                &path,
                base_dir,
                output,
                static_files,
                module_map,
                next_indent,
            )?;
        }
    }

    if create_module {
        let indent = "    ".repeat(indent_level);
        output.push_str(&format!("{}}}\n", indent));
    }

    Ok(())
}

fn process_file(
    path: &Path,
    base_dir: &Path,
    output: &mut String,
    static_files: &mut Vec<String>,
    module_map: &mut HashMap<String, Vec<String>>,
    indent_level: usize,
) -> std::io::Result<()> {
    let full_path = fs::canonicalize(&path)?;
    let file_name = full_path.to_str().unwrap();

    let hash = calculate_hash(&path)?;

    let var_name = path
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .replace(['/', '.', '-'], "_");

    let file_stem = path.file_stem().unwrap().to_str().unwrap();
    let extension = path.extension().unwrap().to_str().unwrap();

    let rel_path = path.strip_prefix(base_dir).unwrap_or(path);
    let rel_dir = rel_path.parent().unwrap_or(Path::new(""));
    let rel_dir_str = rel_dir.to_str().unwrap().replace('\\', "/");

    let url_path = if rel_dir_str.is_empty() {
        format!("/static/{file_stem}-{hash}.{extension}")
    } else {
        format!("/static/{rel_dir_str}/{file_stem}-{hash}.{extension}")
    };

    let mime_type = mime_type_from_extension(extension);

    let module_path = if rel_dir.to_str().unwrap().is_empty() {
        "root".to_string()
    } else {
        get_module_path(rel_dir)
    };

    let indent = "    ".repeat(indent_level);

    let file_code = format!(
        r#"
{indent}/// From "{file_name}"
{indent}#[allow(non_upper_case_globals)]
{indent}pub static {var_name}: StaticFile = StaticFile {{
{indent}    file_name: "{file_name}",
{indent}    name: "{url_path}",
{indent}    mime: "{mime_type}",
{indent}}};
"#,
    );

    output.push_str(&file_code);

    if module_path == "root" {
        static_files.push(format!("&{}", var_name));
    } else {
        let module_parts: Vec<&str> = module_path.split('/').collect();
        let qualified_path = if module_parts.len() > 1 {
            let mut parts = Vec::new();
            for part in &module_parts {
                if !part.is_empty() {
                    parts.push(*part);
                }
            }
            format!("&{}::{}", parts.join("::"), var_name)
        } else {
            format!("&{}::{}", module_path, var_name)
        };

        static_files.push(qualified_path);

        if let Some(vars) = module_map.get_mut(&module_path) {
            vars.push(var_name);
        }
    }

    Ok(())
}

fn get_module_path(path: &Path) -> String {
    path.to_str()
        .unwrap()
        .replace('\\', "/")
        .replace(['.', '-'], "_")
}

fn calculate_hash(path: &Path) -> std::io::Result<String> {
    let mut file = File::open(path)?;
    let mut buffer = Vec::new();

    file.read_to_end(&mut buffer)?;

    let hash = md5::compute(&buffer);
    Ok(format!("{:x}", hash))
}

fn mime_type_from_extension(extension: &str) -> &'static str {
    match extension {
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "css" => "text/css",
        "js" => "application/javascript",
        "wasm" => "application/wasm",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;
    use tempfile::{tempdir, NamedTempFile};

    fn create_temp_file(content: &[u8], extension: &str) -> (NamedTempFile, PathBuf) {
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(content).unwrap();
        let path = file.path().to_path_buf();

        let new_path = path.with_extension(extension);
        fs::rename(&path, &new_path).unwrap();

        (file, new_path)
    }

    #[test]
    fn test_basic_file_processing() {
        let dir = tempdir().unwrap();
        let out_path = dir.path().join("static_gen.rs");

        let test_content = b"test content";
        let (_file, file_path) = create_temp_file(test_content, "js");

        codegen(&out_path, &[], &[file_path.clone()]).unwrap();

        let generated = fs::read_to_string(&out_path).unwrap();

        assert!(generated.contains("pub struct StaticFile"));
        assert!(generated.contains(&format!("file_name: \"{}\"", file_path.to_str().unwrap())));
        assert!(generated.contains("pub static STATICS: &[&StaticFile]"));

        let hash = format!("{:x}", md5::compute(test_content));
        assert!(generated.contains(&hash));
    }

    #[test]
    fn test_directory_structure() {
        let dir = tempdir().unwrap();
        let out_path = dir.path().join("static_gen.rs");

        let nested_dir = dir.path().join("vendor");
        fs::create_dir(&nested_dir).unwrap();

        let root_content = b"root file";
        let nested_content = b"nested file";

        let (_root_file, root_path) = create_temp_file(root_content, "css");
        fs::rename(&root_path, dir.path().join("root.css")).unwrap();

        let nested_path = nested_dir.join("script.js");
        let mut nested_file = File::create(&nested_path).unwrap();
        nested_file.write_all(nested_content).unwrap();

        codegen(&out_path, &[dir.path().to_path_buf()], &[]).unwrap();

        let generated = fs::read_to_string(&out_path).unwrap();

        assert!(generated.contains("pub mod vendor {"));
        assert!(generated.contains("pub static root_css"));
        assert!(generated.contains("pub static script_js"));

        assert!(generated.contains("\"/static/root-"));
        assert!(generated.contains("\"/static/vendor/script-"));

        assert!(generated.contains("&root_css"));
        assert!(generated.contains("&vendor::script_js"));
    }

    #[test]
    fn test_mime_type_detection() {
        let dir = tempdir().unwrap();
        let out_path = dir.path().join("static_gen.rs");

        let extensions = vec!["svg", "png", "jpg", "css", "js", "wasm", "webp", "unknown"];
        let mut file_paths = Vec::new();

        for ext in extensions {
            let (_file, path) = create_temp_file(format!("content for {}", ext).as_bytes(), ext);
            file_paths.push(path);
        }

        codegen(&out_path, &[], &file_paths).unwrap();

        let generated = fs::read_to_string(&out_path).unwrap();

        assert!(generated.contains("mime: \"image/svg+xml\""));
        assert!(generated.contains("mime: \"image/png\""));
        assert!(generated.contains("mime: \"image/jpeg\""));
        assert!(generated.contains("mime: \"text/css\""));
        assert!(generated.contains("mime: \"application/javascript\""));
        assert!(generated.contains("mime: \"application/wasm\""));
        assert!(generated.contains("mime: \"image/webp\""));
        assert!(generated.contains("mime: \"application/octet-stream\""));
    }

    #[test]
    fn test_display_implementation() {
        let dir = tempdir().unwrap();
        let out_path = dir.path().join("static_gen.rs");

        let test_content = b"test content for display";
        let (_file, file_path) = create_temp_file(test_content, "txt");

        codegen(&out_path, &[], &[file_path]).unwrap();

        let generated = fs::read_to_string(&out_path).unwrap();
        assert!(generated.contains("impl std::fmt::Display for StaticFile"));
        assert!(generated
            .contains("fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result"));
    }

    #[test]
    fn test_get_method() {
        let dir = tempdir().unwrap();
        let out_path = dir.path().join("static_gen.rs");

        let test_content = b"test content for get method";
        let (_file, file_path) = create_temp_file(test_content, "png");

        codegen(&out_path, &[], &[file_path]).unwrap();

        let generated = fs::read_to_string(&out_path).unwrap();
        assert!(generated.contains("pub fn get(name: &str) -> Option<&'static Self>"));
    }
}
