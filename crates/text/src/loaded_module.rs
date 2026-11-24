//! Locates a loaded file by address; this does not authenticate loaded bytes or retain module lifetime.

use std::ffi::c_void;
use std::path::PathBuf;

pub fn path_for_address(address: *const c_void) -> Result<PathBuf, String> {
    if address.is_null() {
        return Err("A loaded module address must not be null".into());
    }
    let path = platform_path(address)?
        .canonicalize()
        .map_err(|error| format!("Cannot resolve the loaded module path: {error}"))?;
    if !path.is_file() {
        return Err("The loaded module path is not a regular file".into());
    }
    Ok(path)
}

#[cfg(target_os = "linux")]
struct Mapping {
    range: String,
    path: PathBuf,
}

#[cfg(target_os = "linux")]
fn field<'a>(line: &mut &'a [u8]) -> Result<&'a [u8], String> {
    *line = line.trim_ascii_start();
    let end = line
        .iter()
        .position(u8::is_ascii_whitespace)
        .unwrap_or(line.len());
    if end == 0 {
        return Err("Incomplete loaded-module mapping".into());
    }
    let (value, rest) = line.split_at(end);
    *line = rest;
    Ok(value)
}

#[cfg(target_os = "linux")]
fn mapping_for_address(data: &[u8], address: usize) -> Result<Mapping, String> {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;
    for mut line in data
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
    {
        let range = std::str::from_utf8(field(&mut line)?).map_err(|error| error.to_string())?;
        let (low, high) = range.split_once('-').ok_or("Invalid loaded-module range")?;
        let low = usize::from_str_radix(low, 16).map_err(|error| error.to_string())?;
        let high = usize::from_str_radix(high, 16).map_err(|error| error.to_string())?;
        if address < low || address >= high {
            continue;
        }
        for _ in 0..3 {
            field(&mut line)?;
        }
        let inode = std::str::from_utf8(field(&mut line)?).map_err(|error| error.to_string())?;
        if inode.parse::<u64>().map_err(|error| error.to_string())? == 0 {
            return Err("The address belongs to an anonymous mapping".into());
        }
        let path = PathBuf::from(OsStr::from_bytes(line.trim_ascii_start()));
        if !path.is_absolute() {
            return Err("The mapped file has no absolute path".into());
        }
        return Ok(Mapping {
            range: range.into(),
            path,
        });
    }
    Err("Cannot find the loaded module mapping".into())
}

#[cfg(target_os = "linux")]
fn fallback_path(path: PathBuf) -> Result<PathBuf, String> {
    use std::os::unix::ffi::OsStrExt;
    if path
        .as_os_str()
        .as_bytes()
        .windows(4)
        .any(|bytes| bytes == b"\\012")
    {
        return Err("The mapped file path has an ambiguous newline escape".into());
    }
    Ok(path)
}

#[cfg(target_os = "linux")]
fn platform_path(address: *const c_void) -> Result<PathBuf, String> {
    let data = std::fs::read("/proc/self/maps").map_err(|error| error.to_string())?;
    let mapping = mapping_for_address(&data, address as usize)?;
    // Kernel links retain byte-exact paths; the checked textual fallback works without extra capabilities.
    match std::fs::read_link(format!("/proc/self/map_files/{}", mapping.range)) {
        Ok(path) => Ok(path),
        Err(_) => fallback_path(mapping.path),
    }
}

#[cfg(windows)]
fn platform_path(address: *const c_void) -> Result<PathBuf, String> {
    use std::os::windows::ffi::OsStringExt;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        #[link_name = "GetModuleHandleExW"]
        fn module_from_address(flags: u32, address: *const u16, module: *mut *mut c_void) -> i32;
        #[link_name = "GetModuleFileNameW"]
        fn module_filename(module: *mut c_void, filename: *mut u16, length: u32) -> u32;
    }
    let mut module = std::ptr::null_mut();
    // SAFETY: FROM_ADDRESS and UNCHANGED_REFCOUNT inspect the address without transferring library ownership.
    if unsafe { module_from_address(6, address.cast(), &mut module) } == 0 {
        return Err("Cannot locate the owning loaded module".into());
    }
    let mut filename = vec![0_u16; 32768];
    // SAFETY: the caller retains the module and this buffer holds its declared capacity.
    let length =
        unsafe { module_filename(module, filename.as_mut_ptr(), filename.len() as u32) } as usize;
    if length == 0 || length >= filename.len() {
        return Err("Cannot read the loaded module path".into());
    }
    Ok(PathBuf::from(std::ffi::OsString::from_wide(
        &filename[..length],
    )))
}

#[cfg(not(any(target_os = "linux", windows)))]
fn platform_path(_address: *const c_void) -> Result<PathBuf, String> {
    Err("Loaded-module lookup is unsupported on this target".into())
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;

    const ROW: &[u8] = b"1000-2000 r-xp 00000000 08:02 42 /tmp/a module.so\n";

    #[test]
    fn spaces_are_part_of_the_filename() {
        assert_eq!(
            mapping_for_address(ROW, 0x1000).unwrap().path,
            PathBuf::from("/tmp/a module.so")
        );
    }

    #[test]
    fn range_end_is_exclusive() {
        assert!(mapping_for_address(ROW, 0x2000).is_err());
    }

    #[test]
    fn malformed_ranges_are_refused() {
        assert!(mapping_for_address(b"wrong r-xp 0 08:02 42 /tmp/module\n", 1).is_err());
    }

    #[test]
    fn anonymous_mappings_are_refused() {
        assert_eq!(
            mapping_for_address(b"1000-2000 rw-p 0 00:00 0 /tmp/anonymous.so\n", 0x1000)
                .err()
                .as_deref(),
            Some("The address belongs to an anonymous mapping")
        );
    }

    #[test]
    fn relative_mapping_paths_are_refused() {
        assert_eq!(
            mapping_for_address(b"1000-2000 r-xp 0 08:02 42 module.so\n", 0x1000)
                .err()
                .as_deref(),
            Some("The mapped file has no absolute path")
        );
    }

    #[test]
    fn ambiguous_text_paths_are_refused() {
        assert!(fallback_path(PathBuf::from("/tmp/a\\012module.so")).is_err());
    }

    #[test]
    fn non_utf8_filenames_are_preserved() {
        use std::os::unix::ffi::OsStrExt;
        let data = b"1000-2000 r-xp 0 08:02 42 /tmp/\xff module.so\n";
        assert_eq!(
            mapping_for_address(data, 0x1001)
                .unwrap()
                .path
                .as_os_str()
                .as_bytes(),
            b"/tmp/\xff module.so"
        );
    }

    #[test]
    fn current_code_resolves_to_the_actual_executable() {
        let address = path_for_address as *const () as *const c_void;
        assert_eq!(
            path_for_address(address).unwrap(),
            std::env::current_exe().unwrap().canonicalize().unwrap()
        );
    }

    #[test]
    fn a_foreign_symbol_resolves_to_its_library() {
        unsafe extern "C" {
            fn puts(value: *const std::ffi::c_char) -> std::ffi::c_int;
        }
        let path = path_for_address(puts as *const () as *const c_void).unwrap();
        assert_ne!(
            path,
            std::env::current_exe().unwrap().canonicalize().unwrap()
        );
    }

    #[test]
    fn null_addresses_are_refused() {
        assert_eq!(
            path_for_address(std::ptr::null()).unwrap_err(),
            "A loaded module address must not be null"
        );
    }
}
