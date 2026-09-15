use serde::{Deserialize, Serialize};
use std::{
    env,
    fs::{self, File},
    io::{BufReader, Read, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};
use tempfile::Builder as TempBuilder;
use walkdir::WalkDir;
use zip::{write::SimpleFileOptions, CompressionMethod, ZipArchive, ZipWriter};

const MANIFEST_NAME: &str = "capcut-package.json";
const MAX_IMPORT_SIZE: u64 = 500 * 1024 * 1024 * 1024;
const MAX_IMPORT_ENTRIES: usize = 100_000;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ProjectInfo {
    name: String,
    path: String,
    size_bytes: u64,
    file_count: u64,
    modified_at: u64,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PackageManifest {
    format_version: u8,
    project_name: String,
    directory_name: String,
    exported_at: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ImportResult {
    project_name: String,
    destination: String,
}

fn error_message(context: &str, error: impl std::fmt::Display) -> String {
    format!("{context}: {error}")
}

fn unix_millis(time: SystemTime) -> u64 {
    time.duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn is_capcut_project(path: &Path) -> bool {
    path.join("draft_content.json").is_file() || path.join("draft_meta_info.json").is_file()
}

fn project_display_name(path: &Path) -> String {
    let metadata_path = path.join("draft_meta_info.json");
    if let Ok(contents) = fs::read_to_string(metadata_path) {
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(&contents) {
            if let Some(name) = value.get("draft_name").and_then(|name| name.as_str()) {
                if !name.trim().is_empty() {
                    return name.trim().to_string();
                }
            }
        }
    }

    path.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("Projeto sem nome")
        .to_string()
}

fn inspect_project(path: &Path) -> Result<ProjectInfo, String> {
    let mut size_bytes = 0_u64;
    let mut file_count = 0_u64;
    let mut modified = UNIX_EPOCH;

    for entry in WalkDir::new(path).follow_links(false) {
        let entry = entry.map_err(|error| error_message("Falha ao ler o projeto", error))?;
        let metadata = entry
            .metadata()
            .map_err(|error| error_message("Falha ao ler metadados", error))?;
        if metadata.is_file() {
            file_count += 1;
            size_bytes = size_bytes.saturating_add(metadata.len());
            if let Ok(timestamp) = metadata.modified() {
                modified = modified.max(timestamp);
            }
        }
    }

    Ok(ProjectInfo {
        name: project_display_name(path),
        path: path.to_string_lossy().into_owned(),
        size_bytes,
        file_count,
        modified_at: unix_millis(modified),
    })
}

fn canonical_project(root: &Path, project: &Path) -> Result<PathBuf, String> {
    let root = root
        .canonicalize()
        .map_err(|error| error_message("Pasta raiz inválida", error))?;
    let project = project
        .canonicalize()
        .map_err(|error| error_message("Projeto não encontrado", error))?;

    if !project.starts_with(&root) || project.parent() != Some(root.as_path()) {
        return Err("O projeto não pertence diretamente à pasta raiz selecionada".into());
    }
    if !is_capcut_project(&project) {
        return Err("A pasta não contém um projeto compatível do CapCut".into());
    }
    Ok(project)
}

#[tauri::command]
fn detect_default_root() -> Option<String> {
    let local_app_data = env::var_os("LOCALAPPDATA")?;
    let base = PathBuf::from(local_app_data)
        .join("CapCut")
        .join("User Data")
        .join("Projects");
    let candidates = [base.join("com.lveditor.draft"), base];

    candidates
        .into_iter()
        .find(|path| path.is_dir())
        .map(|path| path.to_string_lossy().into_owned())
}

#[tauri::command]
fn list_projects(root: String) -> Result<Vec<ProjectInfo>, String> {
    let root = PathBuf::from(root);
    if !root.is_dir() {
        return Err("A pasta raiz não existe ou não está acessível".into());
    }

    let mut projects = Vec::new();
    let entries = fs::read_dir(&root)
        .map_err(|error| error_message("Não foi possível abrir a pasta raiz", error))?;
    for entry in entries {
        let entry = entry.map_err(|error| error_message("Falha ao ler uma pasta", error))?;
        let path = entry.path();
        if path.is_dir() && is_capcut_project(&path) {
            projects.push(inspect_project(&path)?);
        }
    }
    projects.sort_by_key(|project| std::cmp::Reverse(project.modified_at));
    Ok(projects)
}

#[tauri::command]
fn export_project(root: String, project: String, destination: String) -> Result<(), String> {
    let project = canonical_project(Path::new(&root), Path::new(&project))?;
    let destination = PathBuf::from(destination);
    if destination.starts_with(&project) {
        return Err("Escolha um destino fora da pasta do projeto".into());
    }
    if destination.exists() {
        return Err("O arquivo de destino já existe".into());
    }
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| error_message("Não foi possível criar o destino", error))?;
    }

    let file = File::create(&destination)
        .map_err(|error| error_message("Não foi possível criar o pacote", error))?;
    let mut archive = ZipWriter::new(file);
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .unix_permissions(0o644);
    let directory_options = SimpleFileOptions::default().unix_permissions(0o755);
    let manifest = PackageManifest {
        format_version: 1,
        project_name: project_display_name(&project),
        directory_name: project
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("Projeto importado")
            .to_string(),
        exported_at: unix_millis(SystemTime::now()),
    };

    let result = (|| -> Result<(), String> {
        archive
            .start_file(MANIFEST_NAME, options)
            .map_err(|error| error_message("Falha ao criar o manifesto", error))?;
        let manifest_json = serde_json::to_vec_pretty(&manifest)
            .map_err(|error| error_message("Falha ao gerar o manifesto", error))?;
        archive
            .write_all(&manifest_json)
            .map_err(|error| error_message("Falha ao gravar o manifesto", error))?;

        for entry in WalkDir::new(&project).follow_links(false) {
            let entry = entry.map_err(|error| error_message("Falha ao ler o projeto", error))?;
            if entry.path_is_symlink() {
                continue;
            }
            let relative = entry
                .path()
                .strip_prefix(&project)
                .map_err(|error| error_message("Caminho inválido no projeto", error))?;
            if relative.as_os_str().is_empty() {
                continue;
            }
            let archive_path = Path::new("project").join(relative);
            let archive_name = archive_path.to_string_lossy().replace('\\', "/");
            if entry.file_type().is_dir() {
                archive
                    .add_directory(format!("{archive_name}/"), directory_options)
                    .map_err(|error| error_message("Falha ao adicionar uma pasta", error))?;
            } else if entry.file_type().is_file() {
                archive
                    .start_file(archive_name, options)
                    .map_err(|error| error_message("Falha ao adicionar um arquivo", error))?;
                let mut source = BufReader::new(
                    File::open(entry.path())
                        .map_err(|error| error_message("Falha ao abrir um arquivo", error))?,
                );
                std::io::copy(&mut source, &mut archive)
                    .map_err(|error| error_message("Falha ao compactar um arquivo", error))?;
            }
        }
        archive
            .finish()
            .map_err(|error| error_message("Falha ao finalizar o pacote", error))?;
        Ok(())
    })();

    if result.is_err() {
        let _ = fs::remove_file(&destination);
    }
    result
}

fn safe_project_name(name: &str) -> String {
    let cleaned: String = name
        .trim()
        .chars()
        .map(|character| {
            if character.is_control() || "<>:\"/\\|?*".contains(character) {
                '_'
            } else {
                character
            }
        })
        .collect();
    let cleaned = cleaned.trim_matches([' ', '.']);
    if cleaned.is_empty() {
        "Projeto importado".into()
    } else {
        cleaned.chars().take(120).collect()
    }
}

fn available_destination(root: &Path, name: &str) -> PathBuf {
    let initial = root.join(name);
    if !initial.exists() {
        return initial;
    }
    for number in 2..10_000 {
        let candidate = root.join(format!("{name} (importado {number})"));
        if !candidate.exists() {
            return candidate;
        }
    }
    root.join(format!(
        "{name} importado {}",
        unix_millis(SystemTime::now())
    ))
}

#[tauri::command]
fn import_project(root: String, package: String) -> Result<ImportResult, String> {
    let root = PathBuf::from(root)
        .canonicalize()
        .map_err(|error| error_message("Pasta raiz inválida", error))?;
    let file = File::open(&package)
        .map_err(|error| error_message("Não foi possível abrir o pacote", error))?;
    let mut archive = ZipArchive::new(file)
        .map_err(|error| error_message("O pacote não é um ZIP válido", error))?;
    if archive.len() > MAX_IMPORT_ENTRIES {
        return Err("O pacote contém arquivos demais".into());
    }
    let mut total_size = 0_u64;
    for index in 0..archive.len() {
        total_size = total_size
            .checked_add(
                archive
                    .by_index(index)
                    .map_err(|error| error_message("Falha ao validar o pacote", error))?
                    .size(),
            )
            .ok_or_else(|| "O tamanho do pacote é inválido".to_string())?;
    }
    if total_size > MAX_IMPORT_SIZE {
        return Err("O pacote descompactado excede o limite de 500 GB".into());
    }

    let manifest: PackageManifest = {
        let mut entry = archive
            .by_name(MANIFEST_NAME)
            .map_err(|_| "O pacote não contém um manifesto válido".to_string())?;
        let mut contents = String::new();
        entry
            .read_to_string(&mut contents)
            .map_err(|error| error_message("Falha ao ler o manifesto", error))?;
        serde_json::from_str(&contents)
            .map_err(|error| error_message("Manifesto inválido", error))?
    };
    if manifest.format_version != 1 {
        return Err(format!(
            "Versão de pacote não suportada: {}",
            manifest.format_version
        ));
    }

    let directory_name = safe_project_name(&manifest.directory_name);
    let destination = available_destination(&root, &directory_name);
    let temporary = TempBuilder::new()
        .prefix(".capcut-import-")
        .tempdir_in(&root)
        .map_err(|error| error_message("Falha ao criar a pasta temporária", error))?;
    let temporary_project = temporary.path().join("project");
    fs::create_dir(&temporary_project)
        .map_err(|error| error_message("Falha ao preparar a importação", error))?;

    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|error| error_message("Falha ao ler o pacote", error))?;
        if entry.name() == MANIFEST_NAME {
            continue;
        }
        if entry
            .unix_mode()
            .is_some_and(|mode| mode & 0o170000 == 0o120000)
        {
            return Err("O pacote contém um link simbólico não permitido".into());
        }
        let enclosed = entry
            .enclosed_name()
            .ok_or_else(|| "O pacote contém um caminho inseguro".to_string())?;
        let relative = enclosed
            .strip_prefix("project")
            .map_err(|_| "O pacote contém arquivos fora da pasta do projeto".to_string())?;
        if relative.as_os_str().is_empty() {
            continue;
        }
        let output = temporary_project.join(relative);
        if entry.is_dir() {
            fs::create_dir_all(&output)
                .map_err(|error| error_message("Falha ao criar uma pasta", error))?;
        } else {
            if let Some(parent) = output.parent() {
                fs::create_dir_all(parent)
                    .map_err(|error| error_message("Falha ao criar uma pasta", error))?;
            }
            let mut output_file = File::create(&output)
                .map_err(|error| error_message("Falha ao criar um arquivo", error))?;
            std::io::copy(&mut entry, &mut output_file)
                .map_err(|error| error_message("Falha ao extrair um arquivo", error))?;
        }
    }

    if !is_capcut_project(&temporary_project) {
        return Err("O pacote não contém os arquivos essenciais de um projeto CapCut".into());
    }
    fs::rename(&temporary_project, &destination)
        .map_err(|error| error_message("Falha ao concluir a importação", error))?;

    Ok(ImportResult {
        project_name: manifest.project_name,
        destination: destination.to_string_lossy().into_owned(),
    })
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            detect_default_root,
            list_projects,
            export_project,
            import_project
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_and_import_preserve_the_project() {
        let workspace = tempfile::tempdir().unwrap();
        let root = workspace.path().join("projects");
        let project = root.join("draft-123");
        fs::create_dir_all(project.join("subfolder")).unwrap();
        fs::write(
            project.join("draft_meta_info.json"),
            r#"{"draft_name":"Viagem"}"#,
        )
        .unwrap();
        fs::write(project.join("subfolder").join("content.bin"), b"capvault").unwrap();
        let package = workspace.path().join("viagem.capcutpkg");

        export_project(
            root.to_string_lossy().into_owned(),
            project.to_string_lossy().into_owned(),
            package.to_string_lossy().into_owned(),
        )
        .unwrap();
        let imported = import_project(
            root.to_string_lossy().into_owned(),
            package.to_string_lossy().into_owned(),
        )
        .unwrap();

        assert_eq!(imported.project_name, "Viagem");
        let destination = PathBuf::from(imported.destination);
        assert_eq!(destination.file_name().unwrap(), "draft-123 (importado 2)");
        assert_eq!(
            fs::read(destination.join("subfolder").join("content.bin")).unwrap(),
            b"capvault"
        );
    }

    #[test]
    fn unsafe_names_are_sanitized() {
        assert_eq!(safe_project_name(" ../Projeto:*? "), "_Projeto___");
        assert_eq!(safe_project_name("..."), "Projeto importado");
    }

    #[test]
    fn invalid_archives_are_rejected() {
        let workspace = tempfile::tempdir().unwrap();
        let package = workspace.path().join("invalid.zip");
        fs::write(&package, b"not a zip").unwrap();

        let result = import_project(
            workspace.path().to_string_lossy().into_owned(),
            package.to_string_lossy().into_owned(),
        );

        assert!(result.is_err());
    }
}
