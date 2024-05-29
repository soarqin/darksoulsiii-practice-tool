use std::env;
use std::fmt::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Instant;

use const_format::formatcp;
use hudhook::tracing::metadata::LevelFilter;
use hudhook::tracing::{debug, error, info};
use hudhook::{ImguiRenderLoop, RenderContext};
use imgui::*;
use libds3::prelude::*;
use pkg_version::*;
use practice_tool_core::crossbeam_channel::{self, Receiver, Sender};
use practice_tool_core::widgets::radial_menu::radial_menu;
use practice_tool_core::widgets::{scaling_factor, Widget, BUTTON_HEIGHT, BUTTON_WIDTH};
use sys::ImVec2;
use tracing_subscriber::prelude::*;
use windows::Win32::UI::Input::XboxController::{
    XINPUT_GAMEPAD_A, XINPUT_GAMEPAD_LEFT_SHOULDER, XINPUT_GAMEPAD_RIGHT_SHOULDER, XINPUT_STATE,
};

use crate::config::{Config, IndicatorType, RadialMenu, Settings};
use crate::{util, XINPUTGETSTATE};

const MAJOR: usize = pkg_version_major!();
const MINOR: usize = pkg_version_minor!();
const PATCH: usize = pkg_version_patch!();

pub(crate) static BLOCK_XINPUT: AtomicBool = AtomicBool::new(false);

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

pub(crate) struct PracticeTool {
    settings: Settings,
    pointers: PointerChains,
    version_label: String,
    widgets: Vec<Box<dyn Widget>>,
    radial_menu: Vec<RadialMenu>,

    log: Vec<(Instant, String)>,
    log_rx: Receiver<String>,
    log_tx: Sender<String>,
    is_hidden: bool,
    ui_state: UiState,
    fonts: Option<FontIDs>,

    position_bufs: [String; 4],
    position_prev: [f32; 3],
    position_change_buf: String,

    igt_buf: String,
    fps_buf: String,

    framecount: u32,
    framecount_buf: String,

    cur_anim_buf: String,

    gamepad_state: XINPUT_STATE,
    gamepad_stick: ImVec2,
    press_queue: Vec<imgui::Key>,
    release_queue: Vec<imgui::Key>,
}

impl PracticeTool {
    pub(crate) fn new() -> Self {
        hudhook::alloc_console().ok();
        log_panics::init();

        fn load_config() -> Result<Config, String> {
            let config_path = util::get_dll_path()
                .map(|mut path| {
                    path.pop();
                    path.push("jdsd_dsiii_practice_tool.toml");
                    path
                })
                .ok_or_else(|| "Couldn't find config file".to_string())?;
            let config_content = std::fs::read_to_string(config_path)
                .map_err(|e| format!("Couldn't read config file: {:?}", e))?;
            println!("{}", config_content);
            Config::parse(&config_content).map_err(String::from)
        }

        let (config, config_err) = match load_config() {
            Ok(config) => (config, None),
            Err(e) => (Config::default(), Some(e)),
        };

        let log_file = util::get_dll_path()
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

        let pointers = PointerChains::new();
        let version_label = {
            let (maj, min, patch) = (*VERSION).into();
            format!("游戏版本 {}.{:02}.{}", maj, min, patch)
        };
        let settings = config.settings.clone();
        let radial_menu = config.radial_menu.clone();
        let widgets = config.make_commands(&pointers);

        let (log_tx, log_rx) = crossbeam_channel::unbounded();
        info!("Initialized");

        PracticeTool {
            settings,
            pointers,
            version_label,
            widgets,
            radial_menu,
            log: Vec::new(),
            log_rx,
            log_tx,
            is_hidden: false,
            ui_state: UiState::Closed,
            fonts: None,
            position_prev: Default::default(),
            position_bufs: Default::default(),
            position_change_buf: Default::default(),
            igt_buf: Default::default(),
            fps_buf: Default::default(),
            framecount: 0,
            framecount_buf: Default::default(),
            cur_anim_buf: Default::default(),
            gamepad_state: Default::default(),
            gamepad_stick: Default::default(),
            press_queue: Vec::new(),
            release_queue: Vec::new(),
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
                if !(ui.io().want_capture_keyboard && ui.is_any_item_active()) {
                    for w in self.widgets.iter_mut() {
                        w.interact(ui);
                    }
                }

                for w in self.widgets.iter_mut() {
                    w.render(ui);
                }

                if ui.button_with_size("关闭", [BUTTON_WIDTH * scaling_factor(ui), BUTTON_HEIGHT])
                {
                    self.ui_state = if self.is_hidden { UiState::Hidden } else { UiState::Closed };
                    self.pointers.cursor_show.set(false);
                }

                if self.is_hidden {
                    if ui.button_with_size("取消隐藏", [
                        BUTTON_WIDTH * scaling_factor(ui),
                        BUTTON_HEIGHT,
                    ]) {
                        self.is_hidden = false;
                        self.ui_state = UiState::Closed;
                        self.pointers.cursor_show.set(false);
                    }
                } else {
                    if ui.button_with_size("卸载", [
                        BUTTON_WIDTH * scaling_factor(ui),
                        BUTTON_HEIGHT,
                    ]) {
                        self.ui_state = UiState::Closed;
                        self.pointers.cursor_show.set(false);
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
                                darksign.icon_id = 5;
                            }
                        }
                        hudhook::eject();
                    }
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
                ui.text("johndisandonato的黑暗之魂III练习工具");

                // ui.same_line();

                if ui.small_button("打开") {
                    self.ui_state = UiState::MenuOpen;
                }

                ui.same_line();

                if ui.small_button("指示器") {
                    ui.open_popup("##indicators_window");
                }

                ui.modal_popup_config("##indicators_window")
                    .resizable(false)
                    .movable(false)
                    .title_bar(false)
                    .build(|| {
                        let style = ui.clone_style();

                        self.pointers.cursor_show.set(true);

                        ui.text(
                            "你可以在这里切换指示器开关，\n重置帧数计数值。\n\n注意，指示器列表和顺序是由\n你的配置文件决定的。",
                        );
                        ui.separator();

                        for indicator in &mut self.settings.indicators {
                            let label = match indicator.indicator {
                                IndicatorType::GameVersion => "游戏版本",
                                IndicatorType::Position => "玩家位置",
                                IndicatorType::PositionChange => "玩家速度",
                                IndicatorType::Igt => "游戏内时间(IGT)",
                                IndicatorType::Fps => "FPS",
                                IndicatorType::FrameCount => "帧数计数器",
                                IndicatorType::ImguiDebug => "ImGui调试信息",
                                IndicatorType::Animation => "动画",
                            };

                            let mut state = indicator.enabled;

                            if ui.checkbox(label, &mut state) {
                                indicator.enabled = state;
                            }

                            if let IndicatorType::FrameCount = indicator.indicator {
                                ui.same_line();

                                let btn_reset_label = "重置";
                                let btn_reset_width = ui.calc_text_size(btn_reset_label)[0]
                                    + style.frame_padding[0] * 2.0;

                                ui.set_cursor_pos([
                                    ui.content_region_max()[0] - btn_reset_width,
                                    ui.cursor_pos()[1],
                                ]);

                                if ui.button("重置") {
                                    self.framecount = 0;
                                }
                            }
                        }

                        ui.separator();

                        let btn_close_width =
                            ui.content_region_max()[0] - style.frame_padding[0] * 2.0;

                        if ui.button_with_size("关闭", [btn_close_width, 0.0]) {
                            ui.close_current_popup();
                            self.pointers.cursor_show.set(false);
                        }
                    });

                ui.same_line();

                if ui.small_button("隐藏")
                {
                    self.is_hidden = true;
                    self.ui_state = UiState::Hidden;
                    self.pointers.cursor_show.set(false);
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
                            MAJOR,
                            MINOR,
                            PATCH
                        ));
                        ui.separator();
                        ui.text(format!(
                            "请按{}键开关工具界面。\n\n你可以点击UI按键或者按下快捷键(方括号内)切换\
                             功能/运行指令\n\n你可以用文本编辑器修改jdsd_dsiii_practice_tool.toml配置\
                             工具的功能。\n如果不小心改坏了配置文件，可以下载原始的配置文件覆盖\n\n\
                             感谢使用我的工具! <3\n",
                            self.settings.display
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
                        ui.same_line();
                        if ui.button("支持作者") {
                            open::that("https://patreon.com/johndisandonato").ok();
                        }
                    });

                ui.new_line();

                for indicator in &self.settings.indicators {
                    if !indicator.enabled {
                        continue;
                    }

                    match indicator.indicator {
                        IndicatorType::GameVersion => {
                            ui.text(&self.version_label);
                        },
                        IndicatorType::Position => {
                            if let (Some([x, y, z]), Some(a)) =
                                (self.pointers.position.1.read(), self.pointers.position.0.read())
                            {
                                self.position_bufs.iter_mut().for_each(String::clear);
                                write!(self.position_bufs[0], "{x:.3}").ok();
                                write!(self.position_bufs[1], "{y:.3}").ok();
                                write!(self.position_bufs[2], "{z:.3}").ok();
                                write!(self.position_bufs[3], "{a:.3}").ok();

                                ui.text_colored(
                                    [0.7048, 0.1228, 0.1734, 1.],
                                    &self.position_bufs[0],
                                );
                                ui.same_line();
                                ui.text_colored(
                                    [0.1161, 0.5327, 0.3512, 1.],
                                    &self.position_bufs[1],
                                );
                                ui.same_line();
                                ui.text_colored(
                                    [0.1445, 0.2852, 0.5703, 1.],
                                    &self.position_bufs[2],
                                );
                                ui.same_line();
                                ui.text(&self.position_bufs[3]);
                            }
                        },
                        IndicatorType::PositionChange => {
                            if let Some([x, y, z]) = self.pointers.position.1.read() {
                                let position_change_xyz = ((x - self.position_prev[0]).powf(2.0)
                                    + (y - self.position_prev[1]).powf(2.0)
                                    + (z - self.position_prev[2]).powf(2.0))
                                .sqrt();

                                let position_change_xz = ((x - self.position_prev[0]).powf(2.0)
                                    + (z - self.position_prev[2]).powf(2.0))
                                .sqrt();

                                let position_change_y = y - self.position_prev[1];

                                self.position_change_buf.clear();
                                write!(
                                    self.position_change_buf,
                                    "[XYZ] {position_change_xyz:.6} | [XZ] \
                                     {position_change_xz:.6} | [Y] {position_change_y:.6}"
                                )
                                .ok();
                                ui.text(&self.position_change_buf);

                                self.position_prev = [x, y, z];
                            }
                        },
                        IndicatorType::Igt => {
                            if let Some(igt) = self.pointers.igt.read() {
                                let millis = (igt % 1000) / 10;
                                let total_seconds = igt / 1000;
                                let seconds = total_seconds % 60;
                                let minutes = total_seconds / 60 % 60;
                                let hours = total_seconds / 3600;
                                self.igt_buf.clear();
                                write!(
                                    self.igt_buf,
                                    "IGT {hours:02}:{minutes:02}:{seconds:02}.{millis:02}",
                                )
                                .ok();
                                ui.text(&self.igt_buf);
                            }
                        },
                        IndicatorType::Fps => {
                            if let Some(fps) = self.pointers.fps.read() {
                                self.fps_buf.clear();
                                write!(self.fps_buf, "FPS {fps}",).ok();
                                ui.text(&self.fps_buf);
                            }
                        },
                        IndicatorType::Animation => {
                            if let (Some(cur_anim), Some(cur_anim_time), Some(cur_anim_length)) = (
                                self.pointers.cur_anim.read(),
                                self.pointers.cur_anim_time.read(),
                                self.pointers.cur_anim_length.read(),
                            ) {
                                self.cur_anim_buf.clear();
                                write!(
                                    self.cur_anim_buf,
                                    "动画 {cur_anim} ({cur_anim_time}s /  {cur_anim_length}s)",
                                )
                                .ok();
                                ui.text(&self.cur_anim_buf);
                            }
                        },
                        IndicatorType::FrameCount => {
                            self.framecount_buf.clear();
                            write!(self.framecount_buf, "帧数 {0}", self.framecount,).ok();
                            ui.text(&self.framecount_buf);
                        },
                        IndicatorType::ImguiDebug => {
                            imgui_debug(ui);
                        },
                    }
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
                    | WindowFlags::NO_INPUTS
            })
            .size([ww, wh], Condition::Always)
            .bg_alpha(0.0)
            .build(|| {
                for _ in 0..5 {
                    ui.text("");
                }
                for l in self.log.iter().rev().take(3).rev() {
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

    fn render_radial(&mut self, ui: &imgui::Ui) {
        let contained_a = self.gamepad_state.Gamepad.wButtons.contains(XINPUT_GAMEPAD_A);
        let [_, h] = ui.io().display_size;
        unsafe { (XINPUTGETSTATE)(0, &mut self.gamepad_state) };

        if self.gamepad_state.Gamepad.wButtons.contains(XINPUT_GAMEPAD_LEFT_SHOULDER)
            && self.gamepad_state.Gamepad.wButtons.contains(XINPUT_GAMEPAD_RIGHT_SHOULDER)
        {
            BLOCK_XINPUT.store(true, Ordering::SeqCst);
            let menu = self
                .radial_menu
                .iter()
                .map(|RadialMenu { label, .. }| label.as_str())
                .collect::<Vec<_>>();
            let x = self.gamepad_state.Gamepad.sThumbLX as f32;
            let y = -(self.gamepad_state.Gamepad.sThumbLY as f32);

            let norm = (x * x + y * y).sqrt();

            if norm > 10000.0 {
                let scale = 1. / norm;
                let x = x * scale;
                let y = y * scale;
                self.gamepad_stick = ImVec2 { x, y };
            }

            let menu_out = radial_menu(ui, &menu, self.gamepad_stick, h * 0.1, h * 0.25);

            if contained_a && !self.gamepad_state.Gamepad.wButtons.contains(XINPUT_GAMEPAD_A) {
                if let Some(i) = menu_out {
                    self.radial_menu[i].key.keys(&mut self.press_queue);
                }
            }
        } else {
            BLOCK_XINPUT.store(false, Ordering::SeqCst);
        }
    }
}

impl ImguiRenderLoop for PracticeTool {
    fn before_render(&mut self, ctx: &mut Context, _: &mut dyn RenderContext) {
        self.release_queue.drain(..).for_each(|key| {
            ctx.io_mut().add_key_event(key, false);
        });
        self.press_queue.drain(..).for_each(|key| {
            ctx.io_mut().add_key_event(key, true);
            self.release_queue.push(key);
        });
    }

    fn render(&mut self, ui: &mut imgui::Ui) {
        let font_token = self.set_font(ui);

        let display = self.settings.display.is_pressed(ui);
        let hide = self.settings.hide.map(|k| k.is_pressed(ui)).unwrap_or(false);

        self.framecount += 1;

        if !ui.io().want_capture_keyboard && (display || hide) {
            self.ui_state = match (&self.ui_state, hide) {
                (UiState::Hidden, _) => UiState::MenuOpen,
                (_, true) => UiState::Hidden,
                (UiState::MenuOpen, _) => if self.is_hidden { UiState::Hidden } else { UiState::Closed },
                (UiState::Closed, _) => UiState::MenuOpen,
            };

            match &self.ui_state {
                UiState::MenuOpen => {},
                UiState::Closed => self.pointers.cursor_show.set(false),
                UiState::Hidden => self.pointers.cursor_show.set(false),
            }
        }

        self.render_radial(ui);

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
            w.log(self.log_tx.clone());
        }

        let now = Instant::now();
        self.log.extend(self.log_rx.try_iter().inspect(|log| info!("{}", log)).map(|l| (now, l)));
        self.log.retain(|(tm, _)| tm.elapsed() < std::time::Duration::from_secs(5));

        self.render_logs(ui);
        drop(font_token);
    }

    fn initialize(&mut self, ctx: &mut Context, _: &mut dyn RenderContext) {
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

        let mut system_font_dir = "C:\\Windows\\Fonts".to_string();
        match env::var_os("windir") {
            Some(x) => {
                let path = PathBuf::from(x).join("Fonts");
                if path.is_dir() {
                    system_font_dir = path.to_str().unwrap().into();
                }
            },
            None => {},
        };
        let mut font_data = Vec::new();
        for filename in ["dengb.ttf", "deng.ttf", "msyh.ttc", "msjhbd.ttc", "msjh.ttc", "simsun.ttc", "mingliub.ttc"] {
            let path = format!("{}\\{}", system_font_dir, filename);
            match std::fs::read(path) {
                Ok(data) => {
                    font_data = data;
                    break;
                },
                Err(_) => {},
            }
        }
        config_big.size_pixels = 24.;
        self.fonts = Some(FontIDs {
            small: fonts.add_font(&[FontSource::TtfData {
                data: &font_data[..],
                size_pixels: 11.,
                config: Some(config_small),
            }]),
            normal: fonts.add_font(&[FontSource::TtfData {
                data: &font_data[..],
                size_pixels: 18.,
                config: Some(config_normal),
            }]),
            big: fonts.add_font(&[FontSource::TtfData {
                data: &font_data[..],
                size_pixels: 24.,
                config: Some(config_big),
            }]),
        });
    }
}

// Display some imgui debug information. Very expensive.
fn imgui_debug(ui: &Ui) {
    let io = ui.io();
    ui.text(format!("Mouse position     {:?}", io.mouse_pos));
    ui.text(format!("Mouse down         {:?}", io.mouse_down));
    ui.text(format!("Want capture mouse {:?}", io.want_capture_mouse));
    ui.text(format!("Want capture kbd   {:?}", io.want_capture_keyboard));
    ui.text(format!("Want text input    {:?}", io.want_text_input));
    ui.text(format!("Want set mouse pos {:?}", io.want_set_mouse_pos));
    ui.text(format!("Any item active    {:?}", ui.is_any_item_active()));
    ui.text(format!("Any item hovered   {:?}", ui.is_any_item_hovered()));
    ui.text(format!("Any item focused   {:?}", ui.is_any_item_focused()));
    ui.text(format!("Any mouse down     {:?}", ui.is_any_mouse_down()));
}
