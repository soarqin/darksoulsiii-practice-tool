#![feature(lazy_cell)]
#![feature(const_fn_floating_point_arithmetic)]

mod config;
mod util;
mod widgets;

use std::ffi::c_void;
use std::sync::Mutex;
use std::thread;
use std::time::Instant;

use const_format::formatcp;
use hudhook::hooks::dx11::ImguiDx11Hooks;
use hudhook::hooks::ImguiRenderLoop;
use hudhook::tracing::metadata::LevelFilter;
use hudhook::tracing::{debug, error, info, trace};
use hudhook::{eject, Hudhook, DLL_PROCESS_ATTACH, HINSTANCE};
use imgui::*;
use libds3::prelude::*;
use pkg_version::*;
use tracing_subscriber::prelude::*;
use widgets::{BUTTON_HEIGHT, BUTTON_WIDTH};
use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_RSHIFT};

const VERSION: (usize, usize, usize) =
    (pkg_version_major!(), pkg_version_minor!(), pkg_version_patch!());

struct FontIDs {
    small: FontId,
    normal: FontId,
    big: FontId,
}

unsafe impl Send for FontIDs {}
unsafe impl Sync for FontIDs {}

enum UiState {
    MenuOpen,
    Closed,
    Hidden,
}

struct PracticeTool {
    config: config::Config,
    widgets: Vec<Box<dyn widgets::Widget>>,
    pointers: PointerChains,
    log: Vec<(Instant, String)>,
    ui_state: UiState,
    fonts: Option<FontIDs>,
}

impl PracticeTool {
    fn new() -> Self {
        hudhook::alloc_console().ok();
        log_panics::init();

        fn load_config() -> Result<config::Config, String> {
            let config_path = crate::util::get_dll_path()
                .map(|mut path| {
                    path.pop();
                    path.push("jdsd_dsiii_practice_tool.toml");
                    path
                })
                .ok_or_else(|| "Couldn't find config file".to_string())?;
            let config_content = std::fs::read_to_string(config_path)
                .map_err(|e| format!("Couldn't read config file: {:?}", e))?;
            println!("{}", config_content);
            config::Config::parse(&config_content).map_err(String::from)
        }

        let (config, config_err) = match load_config() {
            Ok(config) => (config, None),
            Err(e) => (config::Config::default(), Some(e)),
        };

        let log_file = crate::util::get_dll_path()
            .map(|mut path| {
                path.pop();
                path.push("jdsd_dsiii_practice_tool.log");
                path
            })
            .map(std::fs::File::create);

        match log_file {
            Some(Ok(log_file)) => {
                let file_layer = tracing_subscriber::fmt::layer()
                    .with_thread_ids(true)
                    .with_file(true)
                    .with_line_number(true)
                    .with_thread_names(true)
                    .with_writer(Mutex::new(log_file))
                    .with_ansi(false)
                    .boxed();
                let stdout_layer = tracing_subscriber::fmt::layer()
                    .with_thread_ids(true)
                    .with_file(true)
                    .with_line_number(true)
                    .with_thread_names(true)
                    .with_ansi(true)
                    .boxed();

                tracing_subscriber::registry()
                    .with(config.settings.log_level.inner())
                    .with(file_layer)
                    .with(stdout_layer)
                    .init();
            },
            e => {
                tracing_subscriber::fmt()
                    .with_max_level(config.settings.log_level.inner())
                    .with_thread_ids(true)
                    .with_file(true)
                    .with_line_number(true)
                    .with_thread_names(true)
                    .with_ansi(true)
                    .init();

                match e {
                    None => error!("Could not construct log file path"),
                    Some(Err(e)) => error!("Could not initialize log file: {:?}", e),
                    _ => unreachable!(),
                }
            },
        }

        if let Some(err) = config_err {
            debug!("{:?}", err);
        }

        if config.settings.log_level.inner() < LevelFilter::DEBUG || !config.settings.show_console {
            hudhook::free_console().ok();
        } else {
            hudhook::enable_console_colors();
        }

        let pointers = PointerChains::new();

        let widgets = config.make_commands(&pointers);

        {
            let mut params = PARAMS.write();
            if let Some(darksign) = wait_option(|| unsafe {
                if let Err(e) = params.refresh() {
                    error!("{}", e);
                }
                params.get_equip_param_goods()
            })
            .find(|i| i.id == 117)
            .and_then(|p| p.param)
            {
                darksign.icon_id = 116;
            }
        }

        info!("Initialized");

        PracticeTool {
            config,
            pointers,
            widgets,
            ui_state: UiState::Closed,
            log: Vec::new(),
            fonts: None,
        }
    }

    fn render_visible(&mut self, ui: &imgui::Ui) {
        ui.window("##tool_window")
            .position([16., 16.], Condition::Always)
            .bg_alpha(0.8)
            .flags({
                WindowFlags::NO_TITLE_BAR
                    | WindowFlags::NO_RESIZE
                    | WindowFlags::NO_MOVE
                    | WindowFlags::NO_SCROLLBAR
                    | WindowFlags::ALWAYS_AUTO_RESIZE
            })
            .build(|| {
                for w in self.widgets.iter_mut() {
                    w.interact(ui);
                }

                for w in self.widgets.iter_mut() {
                    w.render(ui);
                }

                if ui.button_with_size("关闭", [
                    BUTTON_WIDTH * widgets::scaling_factor(ui),
                    BUTTON_HEIGHT,
                ]) {
                    self.ui_state = UiState::Closed;
                    self.pointers.cursor_show.set(false);
                }

                if option_env!("CARGO_XTASK_DIST").is_none()
                    && ui.button_with_size("卸载工具", [
                        BUTTON_WIDTH * widgets::scaling_factor(ui),
                        BUTTON_HEIGHT,
                    ])
                {
                    self.ui_state = UiState::Closed;
                    self.pointers.cursor_show.set(false);
                    hudhook::eject();
                }
            });
    }

    fn render_closed(&mut self, ui: &imgui::Ui) {
        let stack_tokens = [
            ui.push_style_var(StyleVar::WindowRounding(0.)),
            ui.push_style_var(StyleVar::FrameBorderSize(0.)),
            ui.push_style_var(StyleVar::WindowBorderSize(0.)),
        ];
        ui.window("##msg_window")
            .position([16., ui.io().display_size[1] * 0.14], Condition::Always)
            .bg_alpha(0.0)
            .flags({
                WindowFlags::NO_TITLE_BAR
                    | WindowFlags::NO_RESIZE
                    | WindowFlags::NO_MOVE
                    | WindowFlags::NO_SCROLLBAR
                    | WindowFlags::ALWAYS_AUTO_RESIZE
            })
            .build(|| {
                ui.text("johndisandonato的黑暗之魂III练习工具已激活");

                ui.same_line();

                if ui.small_button("打开") {
                    self.ui_state = UiState::MenuOpen;
                }

                ui.same_line();

                if ui.small_button("帮助") {
                    ui.open_popup("##help_window");
                }

                ui.modal_popup_config("##help_window")
                    .resizable(false)
                    .movable(false)
                    .title_bar(false)
                    .build(|| {
                        self.pointers.cursor_show.set(true);
                        ui.text(formatcp!(
                            "黑暗之魂III练习工具 v{}.{}.{}",
                            VERSION.0,
                            VERSION.1,
                            VERSION.2
                        ));
                        ui.separator();
                        ui.text(format!(
                            "请按{}键开关工具界面。\n\n你可以点击UI按键或者按下快捷键(方括号内)切换\
                             功能/运行指令\n\n你可以用文本编辑器修改jdsd_dsiii_practice_tool.toml配置\
                             工具的功能。\n如果不小心改坏了配置文件，可以下载原始的配置文件覆盖\n\n\
                             感谢使用我的工具! <3\n",
                            self.config.settings.display
                        ));
                        ui.separator();
                        ui.text("-- johndisandonato");
                        ui.text("   https://twitch.tv/johndisandonato");
                        if ui.is_item_clicked() {
                            open::that("https://twitch.tv/johndisandonato").ok();
                        }
                        ui.separator();
                        if ui.button("关闭") {
                            ui.close_current_popup();
                            self.pointers.cursor_show.set(false);
                        }
                        ui.same_line();
                        if ui.button("提交问题反馈(请使用英文)") {
                            open::that(
                                "https://github.com/veeenu/darksoulsiii-practice-tool/issues/new",
                            )
                            .ok();
                        }
                    });

                if let Some(igt) = self.pointers.igt.read() {
                    let millis = (igt % 1000) / 10;
                    let total_seconds = igt / 1000;
                    let seconds = total_seconds % 60;
                    let minutes = total_seconds / 60 % 60;
                    let hours = total_seconds / 3600;
                    ui.text(format!(
                        "游戏内时间 {:02}:{:02}:{:02}.{:02}",
                        hours, minutes, seconds, millis
                    ));
                }

                for w in self.widgets.iter_mut() {
                    w.render_closed(ui);
                }

                for w in self.widgets.iter_mut() {
                    w.interact(ui);
                }
            });

        for st in stack_tokens.into_iter().rev() {
            st.pop();
        }
    }

    fn render_hidden(&mut self, ui: &imgui::Ui) {
        for w in self.widgets.iter_mut() {
            w.interact(ui);
        }
    }

    fn render_logs(&mut self, ui: &imgui::Ui) {
        let io = ui.io();

        let [dw, dh] = io.display_size;
        let [ww, wh] = [dw * 0.3, 14.0 * 6.];

        let stack_tokens = vec![
            ui.push_style_var(StyleVar::WindowRounding(0.)),
            ui.push_style_var(StyleVar::FrameBorderSize(0.)),
            ui.push_style_var(StyleVar::WindowBorderSize(0.)),
        ];

        ui.window("##logs")
            .position_pivot([1., 1.])
            .position([dw * 0.95, dh * 0.8], Condition::Always)
            .flags({
                WindowFlags::NO_TITLE_BAR
                    | WindowFlags::NO_RESIZE
                    | WindowFlags::NO_MOVE
                    | WindowFlags::NO_SCROLLBAR
                    | WindowFlags::ALWAYS_AUTO_RESIZE
            })
            .size([ww, wh], Condition::Always)
            .bg_alpha(0.0)
            .build(|| {
                for _ in 0..20 {
                    ui.text("");
                }
                for l in self.log.iter() {
                    ui.text(&l.1);
                }
                ui.set_scroll_here_y();
            });

        for st in stack_tokens.into_iter().rev() {
            st.pop();
        }
    }

    fn set_font<'a>(&mut self, ui: &'a imgui::Ui) -> imgui::FontStackToken<'a> {
        let width = ui.io().display_size[0];
        let font_id = self
            .fonts
            .as_mut()
            .map(|fonts| {
                if width > 2000. {
                    fonts.big
                } else if width > 1200. {
                    fonts.normal
                } else {
                    fonts.small
                }
            })
            .unwrap();

        ui.push_font(font_id)
    }
}

impl ImguiRenderLoop for PracticeTool {
    fn render(&mut self, ui: &mut imgui::Ui) {
        let font_token = self.set_font(ui);

        if !ui.io().want_capture_keyboard && self.config.settings.display.keyup(ui) {
            let rshift = unsafe { GetAsyncKeyState(VK_RSHIFT.0 as _) < 0 };

            self.ui_state = match (&self.ui_state, rshift) {
                (UiState::Hidden, _) => UiState::Closed,
                (_, true) => UiState::Hidden,
                (UiState::MenuOpen, _) => UiState::Closed,
                (UiState::Closed, _) => UiState::MenuOpen,
            };

            match &self.ui_state {
                UiState::MenuOpen => {},
                UiState::Closed => self.pointers.cursor_show.set(false),
                UiState::Hidden => self.pointers.cursor_show.set(false),
            }
        }

        match &self.ui_state {
            UiState::MenuOpen => {
                self.pointers.cursor_show.set(true);
                self.render_visible(ui);
            },
            UiState::Closed => {
                self.render_closed(ui);
            },
            UiState::Hidden => {
                self.render_hidden(ui);
            },
        }

        for w in &mut self.widgets {
            if let Some(logs) = w.log() {
                let now = Instant::now();
                self.log.extend(logs.into_iter().map(|l| (now, l)));
            }
            self.log.retain(|(tm, _)| tm.elapsed() < std::time::Duration::from_secs(5));
        }

        self.render_logs(ui);
        drop(font_token);
    }

    fn initialize(&mut self, ctx: &mut imgui::Context) {
        let fonts = ctx.fonts();
        let config_small = FontConfig {
            size_pixels: 11.,
            oversample_h: 2,
            oversample_v: 1,
            pixel_snap_h: false,
            glyph_extra_spacing: [0., 0.],
            glyph_offset: [0., 0.],
            glyph_ranges: imgui::FontGlyphRanges::chinese_full(),
            glyph_min_advance_x: 0.,
            glyph_max_advance_x: f32::MAX,
            font_builder_flags: 0,
            rasterizer_multiply: 1.,
            ellipsis_char: None,
            name: Some(String::from("WenQuanYiMicroHeiMono")),
        };
        let mut config_normal = config_small.clone();
        config_normal.size_pixels = 18.;
        let mut config_big = config_small.clone();
        config_big.size_pixels = 24.;
        self.fonts = Some(FontIDs {
            small: fonts.add_font(&[FontSource::TtfData {
                data: include_bytes!("../../lib/data/WenQuanYiMicroHeiMono.ttf"),
                size_pixels: 11.,
                config: Some(config_small),
            }]),
            normal: fonts.add_font(&[FontSource::TtfData {
                data: include_bytes!("../../lib/data/WenQuanYiMicroHeiMono.ttf"),
                size_pixels: 18.,
                config: Some(config_normal),
            }]),
            big: fonts.add_font(&[FontSource::TtfData {
                data: include_bytes!("../../lib/data/WenQuanYiMicroHeiMono.ttf"),
                size_pixels: 24.,
                config: Some(config_big),
            }]),
        });
    }

    fn should_block_messages(&self, _: &Io) -> bool {
        match &self.ui_state {
            UiState::MenuOpen => true,
            UiState::Closed => false,
            UiState::Hidden => false,
        }
    }
}

#[no_mangle]
#[allow(clippy::missing_safety_doc)]
pub unsafe extern "stdcall" fn DllMain(hmodule: HINSTANCE, reason: u32, _: *mut c_void) {
    if reason == DLL_PROCESS_ATTACH {
        trace!("DllMain()");
        thread::spawn(move || {
            let practice_tool = PracticeTool::new();

            if let Err(e) = Hudhook::builder()
                .with(practice_tool.into_hook::<ImguiDx11Hooks>())
                .with_hmodule(hmodule)
                .build()
                .apply()
            {
                error!("Couldn't apply hooks: {e:?}");
                eject();
            }
        });
    }
}
