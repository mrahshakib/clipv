//! Windows 11 Fluent styled UI implementation for Clipboard Manager.
//!
//! Provides a frameless floating popup with rounded corners, dark theme styling,
//! auto-focus search bar, card animations, image thumbnails, pins, and auto-paste interaction.

use crate::clipboard::{copy_item_to_clipboard, simulate_auto_paste};
use crate::db::{ClipboardItem, ContentType, Database};
use crossbeam_channel::Receiver;
use iced::border::Radius;
use iced::keyboard::key::Named;
use iced::keyboard::Key;
use iced::widget::{
    button, column, container, image, row, scrollable, text, text_input, Id, Space,
};
use iced::window::{self, Mode};
use iced::{
    Alignment, Background, Border, Color, Element, Event, Length, Shadow,
    Subscription, Task,
};
use std::sync::Arc;
use std::time::{Duration, Instant};

const SEARCH_INPUT_ID: &str = "clipboard_search_input";
const SCROLLABLE_ID: &str = "clipboard_scrollable";

// Windows 11 Fluent Dark Palette
const BG_DARK: Color = Color::from_rgb(0.12, 0.12, 0.12);         // #1f1f1f
const CARD_BG: Color = Color::from_rgb(0.17, 0.17, 0.17);         // #2b2b2b
const CARD_HOVER_BG: Color = Color::from_rgb(0.22, 0.22, 0.22);   // #383838
const BORDER_COLOR: Color = Color::from_rgb(0.24, 0.24, 0.24);    // #3c3c3c
const ACCENT_COLOR: Color = Color::from_rgb(0.0, 0.47, 0.83);     // #0078d4
const TEXT_MUTED: Color = Color::from_rgb(0.65, 0.65, 0.65);      // #a6a6a6
const PINNED_BADGE: Color = Color::from_rgb(0.10, 0.35, 0.60);    // Pinned highlight

/// Reliably forces X11 / Cinnamon window manager to activate and focus the Clipboard History window,
/// while keeping it hidden from the taskbar, dock, and Alt+Tab window pager.
pub fn activate_window_focus() {
    std::thread::spawn(|| {
        for delay in [15, 50, 100] {
            std::thread::sleep(Duration::from_millis(delay));
            let _ = std::process::Command::new("wmctrl")
                .args(["-r", "Clipboard History", "-b", "add,skip_taskbar,skip_pager"])
                .output();
            let _ = std::process::Command::new("xprop")
                .args([
                    "-name",
                    "Clipboard History",
                    "-f",
                    "_NET_WM_WINDOW_TYPE",
                    "32a",
                    "-set",
                    "_NET_WM_WINDOW_TYPE",
                    "_NET_WM_WINDOW_TYPE_UTILITY",
                ])
                .output();
            let _ = std::process::Command::new("wmctrl")
                .args(["-a", "Clipboard History"])
                .output();
        }
    });
}

/// Messages handled by the UI event loop.
#[derive(Debug, Clone)]
pub enum Message {
    /// Window event from window::events()
    WindowEvent(window::Id, window::Event),
    /// Search input query modified
    SearchChanged(String),
    /// Search input clear button pressed
    ClearSearch,
    /// Periodic tick for external channels (hotkeys, tray, clipboard changes)
    Tick,
    /// Clipboard database modified externally
    #[allow(dead_code)]
    ClipboardUpdated,
    /// Global hotkey Super+V triggered
    HotkeyTriggered,
    /// Card selected by user: copy to clipboard & simulate Ctrl+V
    SelectAndPaste(ClipboardItem),
    /// Toggle pin state of an item
    TogglePin(i64),
    /// Delete single item
    DeleteItem(i64),
    /// Request clear unpinned confirmation banner
    RequestClearAll,
    /// Confirm and execute clear unpinned
    ConfirmClearAll,
    /// Cancel clear unpinned banner
    CancelClearAll,
    /// Hide clipboard popup window
    HideWindow,
    /// Show and focus clipboard popup window
    ShowWindow,
    /// Focus the search input widget
    #[allow(dead_code)]
    FocusSearchInput,
    /// Navigate selection to next item (Down Arrow / Tab)
    SelectNext,
    /// Navigate selection to previous item (Up Arrow / Shift+Tab)
    SelectPrev,
    /// Paste currently selected or first item (Enter)
    SubmitSelection,
    /// Delete currently selected item (Delete key)
    DeleteSelected,
}

/// Main application state.
pub struct ClipboardApp {
    db: Arc<Database>,
    clipboard_rx: Receiver<()>,
    hotkey_rx: Receiver<()>,
    tray_clear_rx: Receiver<()>,
    window_id: Option<window::Id>,
    is_visible: bool,
    is_focused: bool,
    last_shown: Instant,
    items: Vec<ClipboardItem>,
    selected_index: Option<usize>,
    search_query: String,
    show_clear_confirm: bool,
}

impl ClipboardApp {
    /// Initializes application state with database and IPC channels.
    pub fn new(
        db: Arc<Database>,
        clipboard_rx: Receiver<()>,
        hotkey_rx: Receiver<()>,
        tray_clear_rx: Receiver<()>,
        start_hidden: bool,
    ) -> (Self, Task<Message>) {
        let items = db.get_items("").unwrap_or_default();

        let app = Self {
            db,
            clipboard_rx,
            hotkey_rx,
            tray_clear_rx,
            window_id: None,
            is_visible: !start_hidden,
            is_focused: !start_hidden,
            last_shown: Instant::now(),
            items,
            selected_index: None,
            search_query: String::new(),
            show_clear_confirm: false,
        };

        let init_task = if start_hidden {
            Task::batch(vec![
                window::latest().map(|opt| {
                    if let Some(id) = opt {
                        Message::WindowEvent(id, window::Event::Opened { position: None, size: iced::Size::ZERO })
                    } else {
                        Message::Tick
                    }
                }),
            ])
        } else {
            activate_window_focus();
            Task::batch(vec![
                window::latest().map(|opt| {
                    if let Some(id) = opt {
                        Message::WindowEvent(id, window::Event::Opened { position: None, size: iced::Size::ZERO })
                    } else {
                        Message::Tick
                    }
                }),
                iced::widget::operation::focus(Id::new(SEARCH_INPUT_ID)),
            ])
        };

        (app, init_task)
    }

    /// Reloads items from the database taking current search query into account.
    fn refresh_items(&mut self) {
        if let Ok(new_items) = self.db.get_items(&self.search_query) {
            self.items = new_items;
        }
    }

    /// Application title.
    pub fn title(&self) -> String {
        "Clipboard History".to_string()
    }

    /// Application update logic.
    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::WindowEvent(id, event) => {
                self.window_id = Some(id);

                match event {
                    window::Event::Focused => {
                        self.is_focused = true;
                        Task::none()
                    }
                    window::Event::Unfocused => {
                        self.is_focused = false;
                        // Auto-hide when losing focus, ignoring spurious unfocus during initial window mapping (first 250ms)
                        if self.is_visible && self.last_shown.elapsed() > Duration::from_millis(250) {
                            Task::done(Message::HideWindow)
                        } else {
                            Task::none()
                        }
                    }
                    window::Event::CloseRequested => {
                        Task::done(Message::HideWindow)
                    }
                    _ => Task::none(),
                }
            }
            Message::SearchChanged(query) => {
                self.search_query = query;
                self.selected_index = None;
                self.refresh_items();
                Task::none()
            }
            Message::ClearSearch => {
                self.search_query.clear();
                self.selected_index = None;
                self.refresh_items();
                iced::widget::operation::focus(Id::new(SEARCH_INPUT_ID))
            }
            Message::Tick => {
                let mut tasks = Vec::new();

                // 1. Drain all pending clipboard daemon updates
                let mut clip_updated = false;
                while self.clipboard_rx.try_recv().is_ok() {
                    clip_updated = true;
                }
                if clip_updated {
                    self.refresh_items();
                }

                // 2. Drain global Super+V hotkey triggers
                let mut hotkey_triggered = false;
                while self.hotkey_rx.try_recv().is_ok() {
                    hotkey_triggered = true;
                }
                if hotkey_triggered {
                    tasks.push(Task::done(Message::HotkeyTriggered));
                }

                // 3. Drain tray clear commands
                let mut tray_cleared = false;
                while self.tray_clear_rx.try_recv().is_ok() {
                    tray_cleared = true;
                }
                if tray_cleared {
                    let _ = self.db.clear_unpinned();
                    self.selected_index = None;
                    self.refresh_items();
                }

                if tasks.is_empty() {
                    Task::none()
                } else {
                    Task::batch(tasks)
                }
            }
            Message::ClipboardUpdated => {
                self.refresh_items();
                Task::none()
            }
            Message::HotkeyTriggered => {
                // If the window is currently visible AND focused, Super+V dismisses it.
                // In all other cases (window hidden, lost focus, or user was working in another app),
                // Super+V MUST ALWAYS show and focus the window immediately on the single press!
                if self.is_visible && self.is_focused && self.last_shown.elapsed() > Duration::from_millis(150) {
                    Task::done(Message::HideWindow)
                } else {
                    Task::done(Message::ShowWindow)
                }
            }
            Message::ShowWindow => {
                self.is_visible = true;
                self.is_focused = true;
                self.last_shown = Instant::now();
                self.selected_index = None;
                self.search_query.clear();
                self.show_clear_confirm = false;
                self.refresh_items();

                activate_window_focus();

                if let Some(id) = self.window_id {
                    Task::batch(vec![
                        window::set_mode(id, Mode::Windowed),
                        window::gain_focus(id),
                        iced::widget::operation::focus(Id::new(SEARCH_INPUT_ID)),
                    ])
                } else {
                    iced::widget::operation::focus(Id::new(SEARCH_INPUT_ID))
                }
            }
            Message::HideWindow => {
                self.is_visible = false;
                self.is_focused = false;
                self.selected_index = None;
                self.show_clear_confirm = false;

                if let Some(id) = self.window_id {
                    window::set_mode(id, Mode::Hidden)
                } else {
                    Task::none()
                }
            }
            Message::SelectAndPaste(item) => {
                // 1. Copy item back to OS clipboard
                let _ = copy_item_to_clipboard(&item);

                // 2. Hide window so previous app regains focus
                self.is_visible = false;
                self.is_focused = false;
                self.selected_index = None;

                // 3. Simulate Ctrl+V into active application
                simulate_auto_paste();

                if let Some(id) = self.window_id {
                    window::set_mode(id, Mode::Hidden)
                } else {
                    Task::none()
                }
            }
            Message::TogglePin(id) => {
                let _ = self.db.toggle_pin(id);
                self.refresh_items();
                Task::none()
            }
            Message::DeleteItem(id) => {
                let _ = self.db.delete_item(id);
                self.refresh_items();
                if let Some(idx) = self.selected_index {
                    if self.items.is_empty() {
                        self.selected_index = None;
                    } else if idx >= self.items.len() {
                        self.selected_index = Some(self.items.len() - 1);
                    }
                }
                Task::none()
            }
            Message::RequestClearAll => {
                self.show_clear_confirm = true;
                Task::none()
            }
            Message::ConfirmClearAll => {
                self.show_clear_confirm = false;
                let _ = self.db.clear_unpinned();
                self.selected_index = None;
                self.refresh_items();
                Task::none()
            }
            Message::CancelClearAll => {
                self.show_clear_confirm = false;
                Task::none()
            }
            Message::FocusSearchInput => {
                iced::widget::operation::focus(Id::new(SEARCH_INPUT_ID))
            }
            Message::SelectNext => {
                if self.items.is_empty() {
                    return Task::none();
                }
                let next_idx = match self.selected_index {
                    None => 0,
                    Some(idx) => {
                        if idx + 1 < self.items.len() {
                            idx + 1
                        } else {
                            idx
                        }
                    }
                };
                self.selected_index = Some(next_idx);

                // Smoothly scroll only when navigating beyond the first 2 visible cards
                if next_idx <= 2 {
                    iced::widget::operation::snap_to(
                        Id::new(SCROLLABLE_ID),
                        iced::widget::operation::RelativeOffset { x: 0.0, y: 0.0 },
                    )
                } else {
                    let total = self.items.len();
                    let ratio = (next_idx - 2) as f32 / (total.saturating_sub(3).max(1) as f32);
                    iced::widget::operation::snap_to(
                        Id::new(SCROLLABLE_ID),
                        iced::widget::operation::RelativeOffset { x: 0.0, y: ratio.clamp(0.0, 1.0) },
                    )
                }
            }
            Message::SelectPrev => {
                if self.items.is_empty() {
                    return Task::none();
                }
                match self.selected_index {
                    None => Task::none(),
                    Some(0) => {
                        self.selected_index = None;
                        iced::widget::operation::focus(Id::new(SEARCH_INPUT_ID))
                    }
                    Some(idx) => {
                        let prev_idx = idx - 1;
                        self.selected_index = Some(prev_idx);

                        if prev_idx <= 2 {
                            iced::widget::operation::snap_to(
                                Id::new(SCROLLABLE_ID),
                                iced::widget::operation::RelativeOffset { x: 0.0, y: 0.0 },
                            )
                        } else {
                            let total = self.items.len();
                            let ratio = (prev_idx - 2) as f32 / (total.saturating_sub(3).max(1) as f32);
                            iced::widget::operation::snap_to(
                                Id::new(SCROLLABLE_ID),
                                iced::widget::operation::RelativeOffset { x: 0.0, y: ratio.clamp(0.0, 1.0) },
                            )
                        }
                    }
                }
            }
            Message::SubmitSelection => {
                if let Some(idx) = self.selected_index {
                    if idx < self.items.len() {
                        return Task::done(Message::SelectAndPaste(self.items[idx].clone()));
                    }
                } else if !self.items.is_empty() {
                    return Task::done(Message::SelectAndPaste(self.items[0].clone()));
                }
                Task::none()
            }
            Message::DeleteSelected => {
                if let Some(idx) = self.selected_index {
                    if idx < self.items.len() {
                        return Task::done(Message::DeleteItem(self.items[idx].id));
                    }
                }
                Task::none()
            }
        }
    }

    /// Application subscription for events, hotkeys, and channels.
    pub fn subscription(&self) -> Subscription<Message> {
        Subscription::batch(vec![
            // Window focus and lifecycle events
            window::events().map(|(id, ev)| Message::WindowEvent(id, ev)),
            // Intercept keyboard shortcuts (Arrow keys, Enter, Esc, Delete, Tab) even when input is focused
            iced::event::listen_with(|event, status, _window_id| {
                if let Event::Keyboard(iced::keyboard::Event::KeyPressed { key, modifiers, .. }) = event {
                    match key {
                        Key::Named(Named::Escape) => Some(Message::HideWindow),
                        Key::Named(Named::ArrowDown) => Some(Message::SelectNext),
                        Key::Named(Named::ArrowUp) => Some(Message::SelectPrev),
                        Key::Named(Named::Tab) => {
                            if modifiers.shift() {
                                Some(Message::SelectPrev)
                            } else {
                                Some(Message::SelectNext)
                            }
                        }
                        Key::Named(Named::Enter) => Some(Message::SubmitSelection),
                        Key::Named(Named::Delete) if status != iced::event::Status::Captured => {
                            Some(Message::DeleteSelected)
                        }
                        _ => None,
                    }
                } else {
                    None
                }
            }),
            // Ultra-responsive, lightweight tick to process IPC channels (hotkey, tray, clipboard) with sub-20ms latency
            iced::time::every(Duration::from_millis(20)).map(|_| Message::Tick),
        ])
    }

    /// Renders the complete Windows 11 style UI view.
    pub fn view(&self) -> Element<'_, Message> {
        let header = self.view_header();
        let search_bar = self.view_search_bar();
        let content_list = self.view_items_list();

        let mut main_col = column![header]
            .spacing(10)
            .padding(14)
            .width(Length::Fill)
            .height(Length::Fill);

        if self.show_clear_confirm {
            main_col = main_col.push(self.view_clear_confirmation());
        }

        main_col = main_col.push(search_bar).push(content_list);

        // Frameless acrylic-like container with rounded borders
        container(main_col)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(|_theme| container::Style {
                text_color: Some(Color::WHITE),
                background: Some(Background::Color(BG_DARK)),
                border: Border {
                    radius: Radius::from(8.0),
                    width: 1.0,
                    color: BORDER_COLOR,
                },
                shadow: Shadow::default(),
                snap: true,
            })
            .into()
    }

    /// Header bar with Windows 11 title and "Clear all" button (English only).
    fn view_header(&self) -> Element<'_, Message> {
        let title_col = column![
            text("Clipboard History").size(15),
            text("Press Super + V to toggle").size(11).color(TEXT_MUTED),
        ]
        .spacing(1);

        let clear_all_btn = button(text("Clear all").size(12))
            .padding([4, 10])
            .style(|_theme, status| button::Style {
                background: match status {
                    button::Status::Hovered => Some(Background::Color(CARD_HOVER_BG)),
                    button::Status::Pressed => Some(Background::Color(CARD_BG)),
                    _ => None,
                },
                text_color: match status {
                    button::Status::Hovered => Color::WHITE,
                    _ => TEXT_MUTED,
                },
                border: Border {
                    radius: Radius::from(6.0),
                    width: 1.0,
                    color: BORDER_COLOR,
                },
                shadow: Shadow::default(),
                snap: true,
            })
            .on_press(Message::RequestClearAll);

        row![title_col, Space::new().width(Length::Fill), clear_all_btn]
            .align_y(Alignment::Center)
            .into()
    }

    /// Confirmation prompt for clearing unpinned items.
    fn view_clear_confirmation(&self) -> Element<'_, Message> {
        let prompt_text = text("Clear unpinned history items?").size(12);

        let confirm_btn = button(text("Clear").size(11))
            .padding([3, 10])
            .style(|_theme, status| button::Style {
                background: match status {
                    button::Status::Hovered => Some(Background::Color(Color::from_rgb(0.8, 0.2, 0.2))),
                    _ => Some(Background::Color(Color::from_rgb(0.65, 0.15, 0.15))),
                },
                text_color: Color::WHITE,
                border: Border {
                    radius: Radius::from(4.0),
                    width: 0.0,
                    color: Color::TRANSPARENT,
                },
                shadow: Shadow::default(),
                snap: true,
            })
            .on_press(Message::ConfirmClearAll);

        let cancel_btn = button(text("Cancel").size(11))
            .padding([3, 10])
            .style(|_theme, status| button::Style {
                background: match status {
                    button::Status::Hovered => Some(Background::Color(CARD_HOVER_BG)),
                    _ => Some(Background::Color(CARD_BG)),
                },
                text_color: TEXT_MUTED,
                border: Border {
                    radius: Radius::from(4.0),
                    width: 1.0,
                    color: BORDER_COLOR,
                },
                shadow: Shadow::default(),
                snap: true,
            })
            .on_press(Message::CancelClearAll);

        let content = row![
            prompt_text,
            Space::new().width(Length::Fill),
            confirm_btn,
            cancel_btn,
        ]
        .spacing(6)
        .align_y(Alignment::Center);

        container(content)
            .padding(8)
            .style(|_theme| container::Style {
                text_color: Some(Color::WHITE),
                background: Some(Background::Color(Color::from_rgb(0.18, 0.13, 0.13))),
                border: Border {
                    radius: Radius::from(8.0),
                    width: 1.0,
                    color: Color::from_rgb(0.45, 0.2, 0.2),
                },
                shadow: Shadow::default(),
                snap: true,
            })
            .into()
    }

    /// Search bar with auto-focus, placeholder, and clear button.
    fn view_search_bar(&self) -> Element<'_, Message> {
        let search_icon = text("🔍").size(12).color(TEXT_MUTED);

        let input = text_input("Type to search...", &self.search_query)
            .id(Id::new(SEARCH_INPUT_ID))
            .size(13)
            .padding([4, 6])
            .on_input(Message::SearchChanged)
            .on_submit(Message::SubmitSelection)
            .style(|_theme, _status| text_input::Style {
                background: Background::Color(Color::TRANSPARENT),
                border: Border::default(),
                icon: TEXT_MUTED,
                placeholder: TEXT_MUTED,
                value: Color::WHITE,
                selection: ACCENT_COLOR,
            });

        let mut search_row = row![search_icon, input]
            .spacing(8)
            .align_y(Alignment::Center);

        if !self.search_query.is_empty() {
            let clear_btn = button(text("✕").size(11))
                .padding([4, 8])
                .style(|_theme, status| button::Style {
                    background: match status {
                        button::Status::Hovered => Some(Background::Color(CARD_HOVER_BG)),
                        _ => None,
                    },
                    text_color: TEXT_MUTED,
                    border: Border::default(),
                    shadow: Shadow::default(),
                    snap: true,
                })
                .on_press(Message::ClearSearch);

            search_row = search_row.push(clear_btn);
        }

        container(search_row)
            .padding([4, 10])
            .style(|_theme| container::Style {
                text_color: Some(Color::WHITE),
                background: Some(Background::Color(Color::from_rgb(0.16, 0.16, 0.16))),
                border: Border {
                    radius: Radius::from(8.0),
                    width: 1.0,
                    color: BORDER_COLOR,
                },
                shadow: Shadow::default(),
                snap: true,
            })
            .into()
    }

    /// Scrollable list of clipboard cards.
    fn view_items_list(&self) -> Element<'_, Message> {
        if self.items.is_empty() {
            return self.view_empty_state();
        }

        let mut cards_col = column![]
            .spacing(8)
            .width(Length::Fill);

        for (idx, item) in self.items.iter().enumerate() {
            let is_selected = self.selected_index == Some(idx);
            cards_col = cards_col.push(self.view_card(item, is_selected));
        }

        scrollable(cards_col)
            .id(Id::new(SCROLLABLE_ID))
            .width(Length::Fill)
            .height(Length::Fill)
            .style(|theme, status| {
                let mut style = scrollable::default(theme, status);
                style.vertical_rail.background = None;
                style.vertical_rail.scroller.background = Color::from_rgba(0.4, 0.4, 0.4, 0.6).into();
                style
            })
            .into()
    }

    /// Renders a single history item card (text or image) with keyboard selection support.
    fn view_card(&self, item: &ClipboardItem, is_selected: bool) -> Element<'_, Message> {
        let is_pinned = item.is_pinned;
        let item_id = item.id;
        let formatted_time = item.formatted_time();

        // Pin Button
        let pin_text = if is_pinned { "📌 Pinned" } else { "📌" };
        let pin_btn = button(text(pin_text).size(11))
            .padding([3, 7])
            .style(move |_theme, status| button::Style {
                background: if is_pinned {
                    Some(Background::Color(PINNED_BADGE))
                } else {
                    match status {
                        button::Status::Hovered => Some(Background::Color(CARD_HOVER_BG)),
                        _ => None,
                    }
                },
                text_color: if is_pinned {
                    Color::WHITE
                } else {
                    TEXT_MUTED
                },
                border: Border {
                    radius: Radius::from(4.0),
                    width: if is_pinned { 1.0 } else { 0.0 },
                    color: if is_pinned { ACCENT_COLOR } else { Color::TRANSPARENT },
                },
                shadow: Shadow::default(),
                snap: true,
            })
            .on_press(Message::TogglePin(item_id));

        // Delete Button
        let delete_btn = button(text("🗑️").size(11))
            .padding([3, 7])
            .style(|_theme, status| button::Style {
                background: match status {
                    button::Status::Hovered => Some(Background::Color(Color::from_rgb(0.35, 0.15, 0.15))),
                    _ => None,
                },
                text_color: TEXT_MUTED,
                border: Border {
                    radius: Radius::from(4.0),
                    width: 0.0,
                    color: Color::TRANSPARENT,
                },
                shadow: Shadow::default(),
                snap: true,
            })
            .on_press(Message::DeleteItem(item_id));

        // Bottom status row (Timestamp + Pin/Delete action buttons)
        let bottom_row = row![
            text(formatted_time).size(11).color(TEXT_MUTED),
            Space::new().width(Length::Fill),
            pin_btn,
            delete_btn,
        ]
        .spacing(4)
        .align_y(Alignment::Center);

        // Body content based on text or image
        let body_content: Element<'_, Message> = match item.content_type {
            ContentType::Text => {
                let preview = item.preview_text.clone();
                text(preview)
                    .size(13)
                    .color(Color::from_rgb(0.92, 0.92, 0.92))
                    .into()
            }
            ContentType::Image => {
                let mut img_col = column![].spacing(6);

                if let Some(ref path) = item.image_path {
                    let img_handle = image::Handle::from_path(path);
                    let img_widget = image(img_handle)
                        .width(Length::Fill)
                        .height(Length::Fixed(110.0));
                    img_col = img_col.push(img_widget);
                }

                if let (Some(w), Some(h)) = (item.image_width, item.image_height) {
                    let badge = container(
                        text(format!("{} × {}", w, h)).size(10).color(Color::WHITE),
                    )
                    .padding([2, 6])
                    .style(|_theme| container::Style {
                        text_color: Some(Color::WHITE),
                        background: Some(Background::Color(Color::from_rgba(0.0, 0.0, 0.0, 0.65))),
                        border: Border {
                            radius: Radius::from(4.0),
                            width: 0.0,
                            color: Color::TRANSPARENT,
                        },
                        shadow: Shadow::default(),
                        snap: true,
                    });
                    img_col = img_col.push(badge);
                }

                img_col.into()
            }
        };

        // Main body is clickable to copy & auto-paste into active application
        let item_clone = item.clone();
        let clickable_body = button(body_content)
            .width(Length::Fill)
            .padding(4)
            .style(|_theme, _status| button::Style {
                background: None,
                text_color: Color::WHITE,
                border: Border::default(),
                shadow: Shadow::default(),
                snap: true,
            })
            .on_press(Message::SelectAndPaste(item_clone));

        let card_inner = column![clickable_body, bottom_row]
            .spacing(8)
            .padding(10);

        // Stable, consistent card container:
        // Background is always consistent dark CARD_BG.
        // Shadow is always subtle dark (zero colored glow).
        // Selection is indicated solely by a clean 1.5px ACCENT_COLOR border.
        container(card_inner)
            .width(Length::Fill)
            .style(move |_theme| container::Style {
                text_color: Some(Color::WHITE),
                background: Some(Background::Color(CARD_BG)),
                border: Border {
                    radius: Radius::from(8.0),
                    width: if is_selected { 1.5 } else { 1.0 },
                    color: if is_selected {
                        ACCENT_COLOR
                    } else if is_pinned {
                        PINNED_BADGE
                    } else {
                        BORDER_COLOR
                    },
                },
                shadow: Shadow::default(),
                snap: true,
            })
            .into()
    }

    /// Placeholder when history is empty or search returns zero items.
    fn view_empty_state(&self) -> Element<'_, Message> {
        let is_searching = !self.search_query.is_empty();

        let icon = if is_searching { "🔍" } else { "📋" };
        let headline = if is_searching {
            "No matching clipboard items"
        } else {
            "Your clipboard history is empty"
        };
        let subtext = if is_searching {
            "Try searching for another keyword."
        } else {
            "Press Ctrl + C anywhere to copy text or images."
        };

        let empty_col = column![
            text(icon).size(36),
            text(headline).size(14),
            text(subtext).size(12).color(TEXT_MUTED),
        ]
        .spacing(8)
        .align_x(Alignment::Center);

        container(empty_col)
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(Alignment::Center)
            .align_y(Alignment::Center)
            .into()
    }
}
