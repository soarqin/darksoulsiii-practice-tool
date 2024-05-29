#![windows_subsystem = "windows"]

use hudhook::inject::Process;
use hudhook::tracing::{error, trace};
use libjdsd_dsiii_practice_tool::{check_update, events};
use practice_tool_core::update::Update;
use practice_tool_core_windows::message_box;
use practice_tool_core_windows::startup::{is_running, request_start};
use tracing_subscriber::filter::LevelFilter;
use windows::Win32::UI::WindowsAndMessaging::{
    IDYES, MB_ICONERROR, MB_ICONINFORMATION, MB_OK, MB_YESNO,
};

fn load_error(e: impl std::fmt::Display) -> String {
    format!(
        concat!("无法将练习工具加载到黑暗之魂 III：{}\n\n", "请确认杀毒软件没有拦截练习工具。",),
        e,
    )
}

fn perform_injection() -> Result<(), String> {
    let process = Process::by_name("DarkSoulsIII.exe").map_err(|e| {
        error!("Could not find process: {e:?}");
        concat!(
            "黑暗之魂 III 尚未运行。请先启动游戏，再运行练习工具。\n\n",
            "如果游戏以管理员身份运行，请同时以管理员身份运行练习工具。",
        )
        .to_string()
    })?;

    let events = events();
    if is_running(&events) {
        return Err(concat!("练习工具已在运行。\n\n", "如需启动另一份练习工具，请先重启游戏。",)
            .to_string());
    }

    let mut dll_path = std::env::current_exe().map_err(load_error)?;
    dll_path.set_file_name("dinput8.dll");

    if !dll_path.exists() {
        return Err(format!(
            concat!(
                "找不到 {}。\n\n",
                "请先解压压缩包中的所有文件，再运行练习工具，并确认杀毒软件没有删除该文件。",
            ),
            dll_path.display()
        ));
    }

    trace!("Injecting {:?}", dll_path);
    process.inject(dll_path).map_err(load_error)?;

    // A freshly loaded DLL is running by now. If it isn't, the game had already
    // loaded this very file at startup, so injecting it did nothing: ask
    // that copy to start instead.
    if !is_running(&events) {
        request_start(&events).map_err(|_| load_error("练习工具未能启动。"))?;
    }

    Ok(())
}

fn main() {
    tracing_subscriber::fmt()
        .with_max_level(LevelFilter::TRACE)
        .with_thread_ids(true)
        .with_file(true)
        .with_line_number(true)
        .with_thread_names(true)
        .init();

    match check_update() {
        Update::Available { url, notes } => {
            let text = format!("{notes}\n是否下载新版本？选择「是」后，练习工具不会启动。");
            if message_box("发现新版本", &text, MB_YESNO | MB_ICONINFORMATION) == IDYES {
                open::that(url).ok();
                return;
            }
        },
        // Shown in the overlay.
        Update::Error(e) => error!("Could not check for updates: {e}"),
        Update::UpToDate => {},
    }

    if let Err(e) = perform_injection() {
        message_box("错误", &e, MB_OK | MB_ICONERROR);
    }
}
