//! The notification-area (system tray) icon: a small menu to bring the control panel back,
//! hide it, show or hide the boxes, or quit. Windows only; elsewhere this does nothing.

use std::sync::mpsc::{Receiver, channel};

/// What the person chose from the tray.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    ShowPanel,
    HidePanel,
    ToggleBoxes,
    Quit,
}

#[cfg(windows)]
pub struct Tray {
    _icon: tray_icon::TrayIcon,
    rx: Receiver<Command>,
}

#[cfg(not(windows))]
pub struct Tray {
    rx: Receiver<Command>,
}

impl Tray {
    /// Put the icon in the tray. `repaint` wakes the app when something is chosen, because a
    /// hidden window would otherwise not look at its messages.
    #[cfg(windows)]
    pub fn new(repaint: impl Fn() + Send + Sync + Clone + 'static) -> Option<Tray> {
        use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
        use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

        let (tx, rx) = channel();

        let (rgba, w, h) = crate::ui_kit::icon_pixels(64)?;
        let icon = Icon::from_rgba(rgba, w, h).ok()?;

        let show = MenuItem::with_id("show", "Show jAyDHD-Grammar-Checker", true, None);
        let hide = MenuItem::with_id("hide", "Hide control panel", true, None);
        let boxes = MenuItem::with_id("boxes", "Show / hide boxes", true, None);
        let quit = MenuItem::with_id("quit", "Quit", true, None);
        let menu = Menu::new();
        menu.append(&show).ok()?;
        menu.append(&hide).ok()?;
        menu.append(&boxes).ok()?;
        menu.append(&PredefinedMenuItem::separator()).ok()?;
        menu.append(&quit).ok()?;

        let menu_tx = tx.clone();
        let menu_repaint = repaint.clone();
        MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
            let command = match event.id.0.as_str() {
                "show" => Command::ShowPanel,
                "hide" => Command::HidePanel,
                "boxes" => Command::ToggleBoxes,
                "quit" => Command::Quit,
                _ => return,
            };
            let _ = menu_tx.send(command);
            menu_repaint();
        }));
        // A left click on the icon brings the control panel back.
        TrayIconEvent::set_event_handler(Some(move |event: TrayIconEvent| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                let _ = tx.send(Command::ShowPanel);
                repaint();
            }
        }));

        let icon = TrayIconBuilder::new()
            .with_tooltip("jAyDHD-Grammar-Checker")
            .with_icon(icon)
            .with_menu(Box::new(menu))
            .with_menu_on_left_click(false)
            .build()
            .ok()?;
        Some(Tray { _icon: icon, rx })
    }

    #[cfg(not(windows))]
    pub fn new(_repaint: impl Fn() + Send + Sync + Clone + 'static) -> Option<Tray> {
        let (_tx, rx) = channel();
        Some(Tray { rx })
    }

    /// Everything chosen since the last call.
    pub fn poll(&self) -> Vec<Command> {
        self.rx.try_iter().collect()
    }
}
