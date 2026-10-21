use std::fs;
use std::io::{self, IsTerminal, Read};
use std::path::{Path, PathBuf};

const SUPPORTED_EXTENSIONS: &[&str] = &[
    "jpg", "jpeg", "png", "webp", "bmp", "tga", "tiff", "tif", "gif", "avif", "qoi",
];

pub fn parse_path_input(input: &str) -> Option<PathBuf> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return None;
    }
    let line = trimmed.lines().next().unwrap_or(trimmed).trim();
    let clean = line.trim_matches(|c| c == '\'' || c == '"').trim();
    if clean.is_empty() {
        return None;
    }
    if let Some(stripped) = clean.strip_prefix("~/")
        && let Ok(home) = std::env::var("HOME")
    {
        return Some(PathBuf::from(home).join(stripped));
    }
    Some(PathBuf::from(clean))
}

/// Read path from stdin if piped (non-terminal).
pub fn read_stdin_path() -> Option<PathBuf> {
    let stdin = io::stdin();
    if stdin.is_terminal() {
        return None;
    }

    let mut buffer = String::new();
    if stdin.lock().read_to_string(&mut buffer).is_ok() {
        return parse_path_input(&buffer);
    }
    None
}

fn is_supported_image(path: &Path) -> bool {
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        let lower = ext.to_ascii_lowercase();
        SUPPORTED_EXTENSIONS.contains(&lower.as_str())
    } else {
        false
    }
}

/// Scan a directory for supported image files.
fn scan_dir(dir: &Path) -> Vec<PathBuf> {
    let mut results = Vec::new();
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() && is_supported_image(&path) {
                results.push(path);
            }
        }
    }
    results
}

/// Find wallpapers and determine the starting index.
pub fn discover_wallpapers(
    cli_args: &[PathBuf],
    stdin_path: Option<&Path>,
) -> Result<(Vec<PathBuf>, usize), String> {
    let mut candidates = Vec::new();

    // 1. Check CLI args
    if !cli_args.is_empty() {
        let first = &cli_args[0];
        if first.is_dir() {
            candidates = scan_dir(first);
        } else {
            for arg in cli_args {
                if arg.is_file() && is_supported_image(arg) {
                    candidates.push(arg.clone());
                } else if arg.is_dir() {
                    candidates.extend(scan_dir(arg));
                }
            }
        }
    }

    // 2. If no CLI candidates, look at stdin path's parent directory
    if candidates.is_empty()
        && let Some(path) = stdin_path
    {
        if path.is_dir() {
            candidates = scan_dir(path);
        } else if let Some(parent) = path.parent()
            && parent.is_dir()
        {
            candidates = scan_dir(parent);
        }
    }

    // 3. If still empty, check common wallpaper directories
    if candidates.is_empty() {
        let mut fallback_dirs = Vec::new();

        if let Ok(env_dir) = std::env::var("WALLPAPERS_DIR") {
            fallback_dirs.push(PathBuf::from(env_dir));
        }
        if let Ok(env_dir) = std::env::var("WALLPAPER_DIR") {
            fallback_dirs.push(PathBuf::from(env_dir));
        }
        if let Ok(home) = std::env::var("HOME") {
            let home_path = PathBuf::from(home);
            fallback_dirs.push(home_path.join("Pictures/Wallpapers"));
            fallback_dirs.push(home_path.join("Pictures/wallpapers"));
            fallback_dirs.push(home_path.join("Pictures"));
            fallback_dirs.push(home_path.join(".local/share/wallpapers"));
        }
        fallback_dirs.push(PathBuf::from("."));
        fallback_dirs.push(PathBuf::from("/usr/share/backgrounds"));

        for dir in fallback_dirs {
            if dir.is_dir() {
                let found = scan_dir(&dir);
                if !found.is_empty() {
                    candidates = found;
                    break;
                }
            }
        }
    }

    if candidates.is_empty() {
        return Err(
            "No wallpaper images found! Please specify a directory as argument or pipe an image path."
                .to_string(),
        );
    }

    // Sort naturally / alphabetically
    candidates.sort_by(|a, b| {
        let a_name = a.file_name().and_then(|n| n.to_str()).unwrap_or("");
        let b_name = b.file_name().and_then(|n| n.to_str()).unwrap_or("");
        a_name.cmp(b_name)
    });

    // Deduplicate
    candidates.dedup();

    // Determine initial index
    let mut initial_index = 0;
    if let Some(target) = stdin_path {
        let target_canon = target.canonicalize().ok();
        let target_name = target.file_name();

        for (idx, item) in candidates.iter().enumerate() {
            if item == target {
                initial_index = idx;
                break;
            }
            if let (Some(tc), Ok(ic)) = (&target_canon, item.canonicalize())
                && tc == &ic
            {
                initial_index = idx;
                break;
            }
            if let (Some(tn), Some(in_name)) = (target_name, item.file_name())
                && tn == in_name
            {
                initial_index = idx;
                break;
            }
        }
    }

    Ok((candidates, initial_index))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_test_wallpapers(test_name: &str) -> PathBuf {
        let temp_dir =
            std::env::temp_dir().join(format!("swall_test_{}_{}", std::process::id(), test_name));
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();
        std::fs::write(temp_dir.join("01_sunset.png"), b"").unwrap();
        std::fs::write(temp_dir.join("02_forest.png"), b"").unwrap();
        std::fs::write(temp_dir.join("03_mountain.png"), b"").unwrap();
        temp_dir
    }

    #[test]
    fn test_discover_with_cli_dir() {
        let dir = setup_test_wallpapers("cli_dir");
        let (walls, start) = discover_wallpapers(&[dir.clone()], None).unwrap();
        assert_eq!(walls.len(), 3);
        assert_eq!(start, 0);
        assert_eq!(
            walls[0].file_name().unwrap().to_str().unwrap(),
            "01_sunset.png"
        );
        assert_eq!(
            walls[1].file_name().unwrap().to_str().unwrap(),
            "02_forest.png"
        );
        assert_eq!(
            walls[2].file_name().unwrap().to_str().unwrap(),
            "03_mountain.png"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_discover_with_stdin_file() {
        let dir = setup_test_wallpapers("stdin_file");
        let file = dir.join("02_forest.png");
        let (walls, start) = discover_wallpapers(&[], Some(&file)).unwrap();
        assert_eq!(walls.len(), 3);
        assert_eq!(start, 1);
        assert_eq!(
            walls[start].file_name().unwrap().to_str().unwrap(),
            "02_forest.png"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_discover_with_stdin_nonexistent() {
        let dir = setup_test_wallpapers("stdin_nonexistent");
        let file = dir.join("99_none.png");
        let (walls, start) = discover_wallpapers(&[], Some(&file)).unwrap();
        assert_eq!(walls.len(), 3);
        // Fallback to first by sort order
        assert_eq!(start, 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_parse_path_input() {
        assert_eq!(parse_path_input(""), None);
        assert_eq!(parse_path_input("   \n\t  "), None);
        assert_eq!(
            parse_path_input("/path/to/wall.jpg\n"),
            Some(PathBuf::from("/path/to/wall.jpg"))
        );
        assert_eq!(
            parse_path_input("  '/home/user/img.png'  "),
            Some(PathBuf::from("/home/user/img.png"))
        );
        assert_eq!(
            parse_path_input("\"/home/user/space name.webp\"\n"),
            Some(PathBuf::from("/home/user/space name.webp"))
        );
    }
}
