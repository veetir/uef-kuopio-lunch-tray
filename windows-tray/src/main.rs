#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

mod antell;
mod api;
mod app;
mod cache;
mod favorites;
mod format;
mod gpu;
mod log;
mod model;
mod popup;
mod restaurant;
mod settings;
mod startup;
mod tray;
mod util;
mod winmsg;

use crate::app::App;
use crate::format::{
    date_and_time_line, menu_heading, normalize_text, split_component_suffix, student_price_eur,
    text_for, PriceGroups,
};
use crate::restaurant::{restaurant_for_code, Provider};
use crate::settings::load_settings;
use crate::util::to_wstring;
use windows::core::PCWSTR;
use windows::Win32::Foundation::HWND;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DispatchMessageW, GetMessageW, TranslateMessage, MSG, SW_HIDE,
    WS_EX_TOOLWINDOW, WS_OVERLAPPEDWINDOW, WS_POPUP,
};
use windows::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONERROR, MB_OK};

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let print_today = args.iter().any(|a| a == "--print-today");
    let no_tray = args.iter().any(|a| a == "--no-tray");
    let boot_settings = load_settings();
    log::set_enabled(boot_settings.enable_logging);

    if print_today {
        ensure_console();
        return print_today_menu_with_settings(&boot_settings);
    }

    unsafe {
        log::log_line("app start");
        let hinstance = GetModuleHandleW(None)?;
        winmsg::register_window_classes(hinstance.into())?;

        let app = Box::new(App::new(no_tray));
        let app_ptr = Box::into_raw(app);

        let tray_class = to_wstring(winmsg::TRAY_WND_CLASS);
        let tray_hwnd = CreateWindowExW(
            Default::default(),
            PCWSTR(tray_class.as_ptr()),
            PCWSTR(to_wstring("Compass Lunch").as_ptr()),
            WS_OVERLAPPEDWINDOW,
            0,
            0,
            0,
            0,
            HWND(0),
            None,
            hinstance,
            Some(app_ptr as *const _ as *const _),
        );

        let popup_class = to_wstring(winmsg::POPUP_WND_CLASS);
        let popup_style = if no_tray {
            WS_OVERLAPPEDWINDOW
        } else {
            WS_POPUP
        };
        let popup_ex_style = if no_tray {
            Default::default()
        } else {
            WS_EX_TOOLWINDOW
        };
        let popup_hwnd = CreateWindowExW(
            popup_ex_style,
            PCWSTR(popup_class.as_ptr()),
            PCWSTR(to_wstring("Compass Lunch").as_ptr()),
            popup_style,
            0,
            0,
            0,
            0,
            HWND(0),
            None,
            hinstance,
            Some(app_ptr as *const _ as *const _),
        );

        if tray_hwnd.0 == 0 || popup_hwnd.0 == 0 {
            log::log_line("failed to create windows");
            return Err(anyhow::anyhow!("Failed to create windows"));
        }

        let app = &*app_ptr;
        app.set_hwnds(tray_hwnd, popup_hwnd);
        let startup_state = app.snapshot();
        if startup_state.settings.renderer_backend == "gpu" {
            if let Err(err) = gpu::probe_hardware() {
                let detail = format!("{:#}", err);
                app.set_renderer_backend("gdi");
                let title = to_wstring("Compass Lunch - GPU Renderer");
                let body = to_wstring(&format!(
                    "GPU renderer could not be initialized on startup.\n\n{}\n\nFalling back to GDI.",
                    detail
                ));
                let _ = MessageBoxW(
                    tray_hwnd,
                    PCWSTR(body.as_ptr()),
                    PCWSTR(title.as_ptr()),
                    MB_OK | MB_ICONERROR,
                );
            }
        }
        let _ = app.load_cache_for_current();
        winmsg::schedule_timers(tray_hwnd, app.refresh_minutes());
        app.check_stale_date_and_refresh();
        app.start_refresh();

        if !no_tray {
            match tray::add_tray_icon(tray_hwnd, winmsg::WM_TRAY_CALLBACK) {
                Ok(()) => log::log_line("tray icon added"),
                Err(err) => {
                    log::log_line(&format!("tray icon add failed: {}", err));
                    return Err(err);
                }
            }
        }

        windows::Win32::UI::WindowsAndMessaging::ShowWindow(tray_hwnd, SW_HIDE);

        if no_tray {
            let state = app.snapshot();
            popup::show_popup(popup_hwnd, &state);
        }

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, HWND(0), 0, 0).into() {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }

    Ok(())
}

#[cfg(target_os = "windows")]
fn ensure_console() {
    use windows::Win32::System::Console::{AllocConsole, AttachConsole, ATTACH_PARENT_PROCESS};
    unsafe {
        if AttachConsole(ATTACH_PARENT_PROCESS).is_err() {
            let _ = AllocConsole();
        }
    }
}

#[cfg(not(target_os = "windows"))]
fn ensure_console() {}

fn print_today_menu_with_settings(settings: &crate::settings::Settings) -> anyhow::Result<()> {
    let result = api::fetch_today(settings);
    if !result.ok {
        eprintln!(
            "{}: {}",
            text_for(&settings.language, "fetchError"),
            result.error_message
        );
        return Ok(());
    }

    let today_menu = result.today_menu;
    let date_line = date_and_time_line(today_menu.as_ref(), &settings.language);
    if !date_line.is_empty() {
        println!("{}", date_line);
    }

    let provider = restaurant_for_code(
        &settings.restaurant_code,
        settings.enable_antell_restaurants,
    )
    .provider;
    let price_groups = PriceGroups {
        student: settings.show_student_price,
        staff: settings.show_staff_price,
        guest: settings.show_guest_price,
    };
    match &today_menu {
        Some(menu) => {
            if !menu.menus.is_empty() {
                for group in &menu.menus {
                    if provider == Provider::Compass && settings.hide_expensive_student_meals {
                        if let Some(price) = student_price_eur(&group.price) {
                            if price > 4.0 {
                                continue;
                            }
                        }
                    }
                    println!(
                        "{}",
                        menu_heading(group, provider, settings.show_prices, price_groups)
                    );
                    for component in &group.components {
                        let component = normalize_text(component);
                        if component.is_empty() {
                            continue;
                        }
                        let (main, suffix) = split_component_suffix(&component);
                        if main.is_empty() {
                            continue;
                        }
                        if !settings.show_allergens || suffix.is_empty() {
                            println!("  ▸ {}", main);
                        } else {
                            println!("  ▸ {} {}", main, suffix);
                        }
                    }
                }
            } else {
                println!("{}", text_for(&settings.language, "noMenu"));
            }
        }
        None => {
            println!("{}", text_for(&settings.language, "noMenu"));
        }
    }

    Ok(())
}
