mod codegen;

use std::env;
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

use widestring::U16CString;
use winapi::ctypes::c_void;
use winapi::shared::minwindef::FALSE;
use winapi::shared::ntdef::{LANG_ENGLISH, MAKELANGID, SUBLANG_DEFAULT};
use winapi::um::winbase::{BeginUpdateResourceW, EndUpdateResourceW, UpdateResourceW};
use winapi::um::winuser::{MAKEINTRESOURCEW, RT_GROUP_ICON, RT_ICON};
use zip::write::FileOptions;
use zip::{CompressionMethod, ZipWriter};

type DynError = Box<dyn std::error::Error>;
type Result<T> = std::result::Result<T, DynError>;

//
// Main
//

fn main() -> Result<()> {
    let task = env::args().nth(1);
    match task.as_deref() {
        Some("dist") => dist()?,
        Some("codegen") => codegen::codegen()?,
        Some("help") => print_help(),
        _ => print_help(),
    }
    Ok(())
}

//
// Tasks
//

fn dist() -> Result<()> {
    let cargo = env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    let status = Command::new(cargo)
        .current_dir(project_root())
        .args(&["build", "--release"])
        .status()
        .map_err(|e| format!("cargo: {}", e))?;

    if !status.success() {
        return Err("cargo build failed".into());
    }

    update_icon(
        project_root().join("target/release/jdsd_dsiii_practice_tool.exe"),
        project_root().join("practice-tool/src/sidherald.ico"),
    )
    .map_err(|e| format!("Update icon: {}", e))?;

    std::fs::remove_dir_all(dist_dir()).ok();
    std::fs::create_dir_all(dist_dir())?;

    let mut zip = ZipWriter::new(File::create(
        dist_dir().join("jdsd_dsiii_practice_tool.zip"),
    )?);
    let file_options = FileOptions::default().compression_method(CompressionMethod::Deflated);

    let mut buf: Vec<u8> = Vec::new();

    let mut add_zip = |src: PathBuf, dst: &str| -> Result<()> {
        File::open(&src)
            .map_err(|e| format!("{}: Couldn't open file: {}", dst, e))?
            .read_to_end(&mut buf)
            .map_err(|e| format!("{}: Couldn't read file: {}", dst, e))?;
        zip.start_file(dst.clone(), file_options)
            .map_err(|e| format!("{}: Couldn't start zip file: {}", dst, e))?;
        zip.write_all(&buf)
            .map_err(|e| format!("{}: Couldn't write zip: {}", dst, e))?;
        buf.clear();
        Ok(())
    };

    add_zip(
        project_root().join("target/release/jdsd_dsiii_practice_tool.exe"),
        "jdsd_dsiii_practice_tool.exe",
    )?;
    add_zip(
        project_root().join("target/release/libjdsd_dsiii_practice_tool.dll"),
        "jdsd_dsiii_practice_tool.dll",
    )?;
    add_zip(
        project_root().join("practice-tool/jdsd_dsiii_practice_tool.toml"),
        "jdsd_dsiii_practice_tool.toml",
    )?;

    Ok(())
}

fn print_help() {
    eprintln!(
        r#"
Tasks:

dist .......... build distribution artifacts
codegen ....... generate Rust code for the parameters
help .......... print this help
"#
    );
}

//
// Utilities
//

fn update_icon(path: PathBuf, icon: PathBuf) -> Result<()> {
    #[repr(C, packed)]
    struct GroupHeader {
        reserved: u16,
        r#type: u16,
        count: u16,
        width: u8,
        height: u8,
        ccount: u8,
        reserved1: u8,
        planes: u16,
        bcount: u16,
        bytes: u32,
        offset: u32,
    }

    let mut buf: Vec<u8> = Vec::new();
    File::open(icon)?.read_to_end(&mut buf)?;

    let mut group_header: &mut GroupHeader = unsafe {
        (buf.as_ptr() as *mut GroupHeader)
            .as_mut()
            .ok_or("Invalid pointer")?
    };

    let start: usize = group_header.offset as usize;
    let count: usize = group_header.bytes as usize;
    let end: usize = start + count;
    let icon_data = &buf[start..end];

    group_header.offset = 1;

    unsafe {
        let handle = BeginUpdateResourceW(
            U16CString::from_str(path.to_str().unwrap())?.as_ptr(),
            FALSE,
        );

        UpdateResourceW(
            handle,
            RT_ICON,
            MAKEINTRESOURCEW(1),
            MAKELANGID(LANG_ENGLISH, SUBLANG_DEFAULT),
            icon_data.as_ptr() as *mut c_void,
            count as u32,
        );

        UpdateResourceW(
            handle,
            RT_GROUP_ICON,
            U16CString::from_str("IDI_ICON")?.as_ptr(),
            MAKELANGID(LANG_ENGLISH, SUBLANG_DEFAULT),
            buf.as_ptr() as *mut c_void,
            std::mem::size_of::<GroupHeader>() as u32,
        );

        EndUpdateResourceW(handle, FALSE);
    }

    Ok(())
}

fn project_root() -> PathBuf {
    Path::new(&env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(1)
        .unwrap()
        .to_path_buf()
}

fn dist_dir() -> PathBuf {
    project_root().join("target/dist")
}