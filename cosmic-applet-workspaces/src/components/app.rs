// Copyright 2023 System76 <info@system76.com>
// SPDX-License-Identifier: GPL-3.0-only

use cctk::{
    cosmic_protocols::toplevel_info::v1::client::zcosmic_toplevel_handle_v1,
    sctk::reexports::{
        calloop::channel::SyncSender,
        protocols::ext::workspace::v1::client::ext_workspace_handle_v1::{
            self, ExtWorkspaceHandleV1,
        },
    },
    toplevel_info::ToplevelInfo,
    wayland_client::protocol::wl_output::WlOutput,
    workspace::Workspace,
};
use cosmic::{
    Element, Task, Theme, app,
    applet::cosmic_panel_config::PanelAnchor,
    desktop::{IconSourceExt, fde},
    iced::core::{Background, Border},
    iced::{
        Alignment,
        Event::Mouse,
        Length, Limits, Padding, Subscription, event,
        mouse::{self, ScrollDelta},
        widget::{Image, Svg, button, column, row, space},
    },
    scroll::DiscreteScrollState,
    surface,
    theme::Container as ContainerClass,
    widget::{Id, autosize, container},
};

use crate::{
    config,
    wayland::WorkspaceEvent,
    wayland_subscription::{WorkspacesUpdate, workspaces},
};

use std::{
    borrow::Cow,
    collections::{HashMap, HashSet},
    process::Command as ShellCommand,
    sync::LazyLock,
    time::Duration,
};

static AUTOSIZE_MAIN_ID: LazyLock<Id> = LazyLock::new(|| Id::new("autosize-main"));

const SCROLL_RATE_LIMIT: Duration = Duration::from_millis(200);
const MAX_VISIBLE_APPS: usize = 5;
const APP_ICON_SPACING: f32 = 3.0;
const APP_GROUP_HORIZONTAL_PADDING: f32 = 6.0;
const APP_GROUP_VERTICAL_PADDING: f32 = 3.0;
const WORKSPACE_CONTENT_SPACING: f32 = 2.0;
const WORKSPACE_LEADING_PADDING: f32 = 5.0;
const WORKSPACE_TRAILING_PADDING: f32 = 8.0;
const WORKSPACE_DIVIDER_WIDTH: f32 = 1.0;
const MINIMIZED_ICON_OPACITY: f32 = 0.45;

pub fn run() -> cosmic::iced::Result {
    cosmic::applet::run::<IcedWorkspacesApplet>(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layout {
    Row,
    Column,
}

struct IcedWorkspacesApplet {
    core: cosmic::app::Core,
    workspaces: Vec<Workspace>,
    toplevels: Vec<ToplevelInfo>,
    output: Option<WlOutput>,
    locales: Vec<String>,
    desktop_entries: Vec<fde::DesktopEntry>,
    app_metadata: HashMap<String, AppMetadata>,
    workspace_tx: Option<SyncSender<WorkspaceEvent>>,
    layout: Layout,
    scroll: DiscreteScrollState,
}

struct AppMetadata {
    name: String,
    icon_source: fde::IconSource,
}

struct WorkspaceApp<'a> {
    app_id: &'a str,
    metadata: &'a AppMetadata,
    window_count: usize,
    minimized_count: usize,
    minimized_titles: Vec<&'a str>,
}

impl WorkspaceApp<'_> {
    fn all_minimized(&self) -> bool {
        self.window_count > 0 && self.minimized_count == self.window_count
    }
}

fn informative_titles<'a>(
    app_name: &str,
    titles: impl IntoIterator<Item = &'a str>,
) -> Vec<&'a str> {
    let app_name = app_name.trim();
    let mut informative = Vec::<&str>::new();

    for title in titles {
        let title = title.trim();
        if title.is_empty()
            || title.eq_ignore_ascii_case(app_name)
            || informative
                .iter()
                .any(|existing| existing.eq_ignore_ascii_case(title))
        {
            continue;
        }
        informative.push(title);
    }

    informative
}

impl IcedWorkspacesApplet {
    /// returns the index of the workspace button after which which must be moved to a popup
    /// if it exists.
    fn popup_index(&self) -> Option<usize> {
        let max_major_axis_len = self.core.applet.suggested_bounds.as_ref().map(|c| {
            // if we have a configure for width and height, we're in a overflow popup
            match self.core.applet.anchor {
                PanelAnchor::Top | PanelAnchor::Bottom => c.width as u32,
                PanelAnchor::Left | PanelAnchor::Right => c.height as u32,
            }
        })?;

        let mut used = 0.0;
        for (index, workspace) in self.workspaces.iter().enumerate() {
            if index > 0 {
                used += WORKSPACE_CONTENT_SPACING;
            }
            used += self.workspace_button_major_size(workspace);
            if used > max_major_axis_len as f32 {
                return Some(index.max(1));
            }
        }

        None
    }

    fn suggested_button_size(&self) -> f32 {
        (self.core.applet.suggested_size(true).0 + self.core.applet.suggested_padding(true).1 * 2)
            as f32
    }

    fn app_icon_size(&self) -> f32 {
        let window_size = self.core.applet.suggested_window_size();
        let cross_axis_size = if self.core.applet.is_horizontal() {
            window_size.1.get() as f32
        } else {
            window_size.0.get() as f32
        };

        (cross_axis_size * 0.52).clamp(16.0, 24.0)
    }

    fn number_section_width(&self, has_apps: bool) -> f32 {
        let base_size = self.suggested_button_size();
        if self.core.applet.is_horizontal() && has_apps {
            (base_size * 0.65).clamp(20.0, 28.0)
        } else {
            base_size
        }
    }

    fn app_group_width(&self, app_count: usize) -> f32 {
        if app_count == 0 {
            return 0.0;
        }

        let visible_count = app_count.min(MAX_VISIBLE_APPS);
        let overflow_width = if app_count > visible_count {
            self.app_icon_size() * 1.15 + APP_ICON_SPACING
        } else {
            0.0
        };

        visible_count as f32 * self.app_icon_size()
            + visible_count.saturating_sub(1) as f32 * APP_ICON_SPACING
            + overflow_width
            + APP_GROUP_HORIZONTAL_PADDING * 2.0
    }

    fn workspace_button_major_size(&self, workspace: &Workspace) -> f32 {
        let base_size = self.suggested_button_size();
        if self.core.applet.is_horizontal() {
            let app_count = self.apps_for_workspace(workspace).len();
            if app_count > 0 {
                WORKSPACE_LEADING_PADDING
                    + WORKSPACE_TRAILING_PADDING
                    + self.number_section_width(true)
                    + WORKSPACE_CONTENT_SPACING * 2.0
                    + WORKSPACE_DIVIDER_WIDTH
                    + self.app_group_width(app_count)
            } else {
                base_size
            }
        } else {
            base_size
        }
    }

    fn update_desktop_entries(&mut self) {
        self.desktop_entries = fde::Iter::new(fde::default_paths())
            .filter_map(|path| fde::DesktopEntry::from_path(path, Some(&self.locales)).ok())
            .collect();
    }

    fn resolve_app_metadata(&mut self, app_id: &str) -> AppMetadata {
        let app_id_key = fde::unicase::Ascii::new(app_id);
        let mut desktop_entry = fde::find_app_by_id(&self.desktop_entries, app_id_key).cloned();

        if desktop_entry.is_none() {
            self.update_desktop_entries();
            desktop_entry = fde::find_app_by_id(&self.desktop_entries, app_id_key).cloned();
        }

        let desktop_entry =
            desktop_entry.unwrap_or_else(|| fde::DesktopEntry::from_appid(app_id.to_owned()));
        let name = desktop_entry
            .full_name(&self.locales)
            .unwrap_or(Cow::Borrowed(&desktop_entry.appid))
            .into_owned();
        let icon_source =
            fde::IconSource::from_unknown(desktop_entry.icon().unwrap_or(&desktop_entry.appid));

        AppMetadata { name, icon_source }
    }

    fn sync_app_metadata(&mut self) {
        let app_ids = self
            .toplevels
            .iter()
            .filter_map(|toplevel| (!toplevel.app_id.is_empty()).then_some(toplevel.app_id.clone()))
            .collect::<HashSet<_>>();

        self.app_metadata
            .retain(|app_id, _| app_ids.contains(app_id));

        for app_id in app_ids {
            if !self.app_metadata.contains_key(&app_id) {
                let metadata = self.resolve_app_metadata(&app_id);
                self.app_metadata.insert(app_id, metadata);
            }
        }
    }

    fn apps_for_workspace(&self, workspace: &Workspace) -> Vec<WorkspaceApp<'_>> {
        let mut apps = Vec::<WorkspaceApp<'_>>::new();

        for toplevel in &self.toplevels {
            if !toplevel.workspace.contains(&workspace.handle) {
                continue;
            }
            if let Some(output) = self.output.as_ref()
                && !toplevel.output.contains(output)
            {
                continue;
            }
            let Some(metadata) = self.app_metadata.get(&toplevel.app_id) else {
                continue;
            };
            let minimized = toplevel
                .state
                .contains(&zcosmic_toplevel_handle_v1::State::Minimized);

            if let Some(app) = apps.iter_mut().find(|app| app.app_id == toplevel.app_id) {
                app.window_count += 1;
                if minimized {
                    app.minimized_count += 1;
                    app.minimized_titles.push(&toplevel.title);
                }
            } else {
                apps.push(WorkspaceApp {
                    app_id: toplevel.app_id.as_str(),
                    metadata,
                    window_count: 1,
                    minimized_count: usize::from(minimized),
                    minimized_titles: minimized
                        .then_some(toplevel.title.as_str())
                        .into_iter()
                        .collect(),
                });
            }
        }

        apps
    }

    fn workspace_tooltip(&self, workspace: &Workspace, apps: &[WorkspaceApp<'_>]) -> String {
        let mut lines = vec![format!(
            "{} · {}",
            workspace.name, self.core.applet.output_name
        )];

        for app in apps {
            let summary = if app.window_count > 1 {
                format!("{} ×{}", app.metadata.name, app.window_count)
            } else {
                app.metadata.name.clone()
            };
            if app.all_minimized() {
                lines.push(format!("{summary} — minimized"));
            } else if app.minimized_count > 0 {
                lines.push(format!("{summary} — {} minimized", app.minimized_count));
            } else {
                lines.push(summary);
            }

            lines.extend(
                informative_titles(&app.metadata.name, app.minimized_titles.iter().copied())
                    .into_iter()
                    .map(|title| {
                        if app.all_minimized() {
                            format!("  ↳ {title}")
                        } else {
                            format!("  ↳ {title} (minimized)")
                        }
                    }),
            );
        }

        lines.join("\n")
    }

    fn app_icon(
        &self,
        metadata: &AppMetadata,
        icon_size: f32,
        minimized: bool,
    ) -> Element<'_, Message> {
        let opacity = if minimized {
            MINIMIZED_ICON_OPACITY
        } else {
            1.0
        };
        let handle = metadata.icon_source.as_cosmic_icon();
        let symbolic = handle.symbolic;

        match handle.data {
            cosmic::widget::icon::Data::Image(handle) => Image::new(handle)
                .width(Length::Fixed(icon_size))
                .height(Length::Fixed(icon_size))
                .opacity(opacity)
                .into(),
            cosmic::widget::icon::Data::Svg(handle) => Svg::<Theme>::new(handle)
                .width(Length::Fixed(icon_size))
                .height(Length::Fixed(icon_size))
                .symbolic(symbolic)
                .opacity(opacity)
                .into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::informative_titles;

    #[test]
    fn hides_titles_that_repeat_the_application_name() {
        assert!(informative_titles("Surfshark", ["Surfshark"]).is_empty());
        assert!(informative_titles("Surfshark", [" surfshark "]).is_empty());
    }

    #[test]
    fn retains_distinct_window_titles_without_duplicates() {
        assert_eq!(
            informative_titles(
                "COSMIC Text Editor",
                ["notes.txt", "NOTES.TXT", "", "README.md"]
            ),
            ["notes.txt", "README.md"]
        );
    }
}

#[derive(Debug, Clone)]
enum Message {
    WorkspaceUpdate(WorkspacesUpdate),
    WorkspacePressed(ExtWorkspaceHandleV1),
    WheelScrolled(ScrollDelta),
    WorkspaceOverview,
    Surface(surface::Action),
}

impl cosmic::Application for IcedWorkspacesApplet {
    type Message = Message;
    type Executor = cosmic::SingleThreadExecutor;
    type Flags = ();
    const APP_ID: &'static str = config::APP_ID;

    fn init(core: cosmic::app::Core, _flags: Self::Flags) -> (Self, app::Task<Self::Message>) {
        let mut app = Self {
            layout: match &core.applet.anchor {
                PanelAnchor::Left | PanelAnchor::Right => Layout::Column,
                PanelAnchor::Top | PanelAnchor::Bottom => Layout::Row,
            },
            core,
            workspaces: Vec::new(),
            toplevels: Vec::new(),
            output: None,
            locales: fde::get_languages_from_env(),
            desktop_entries: Vec::new(),
            app_metadata: HashMap::new(),
            workspace_tx: Option::default(),
            scroll: DiscreteScrollState::default().rate_limit(Some(SCROLL_RATE_LIMIT)),
        };
        app.update_desktop_entries();

        (app, Task::none())
    }

    fn core(&self) -> &cosmic::app::Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut cosmic::app::Core {
        &mut self.core
    }

    fn update(&mut self, message: Self::Message) -> app::Task<Self::Message> {
        match message {
            Message::WorkspaceUpdate(msg) => match msg {
                WorkspacesUpdate::Snapshot(mut snapshot) => {
                    snapshot
                        .workspaces
                        .retain(|w| !w.state.contains(ext_workspace_handle_v1::State::Hidden));
                    snapshot
                        .workspaces
                        .sort_by(|w1, w2| w1.coordinates.cmp(&w2.coordinates));
                    self.workspaces = snapshot.workspaces;
                    self.toplevels = snapshot.toplevels;
                    self.output = snapshot.output;
                    self.sync_app_metadata();
                }
                WorkspacesUpdate::Started(tx) => {
                    self.workspace_tx.replace(tx);
                }
                WorkspacesUpdate::Errored => {
                    // TODO
                }
            },
            Message::WorkspacePressed(id) => {
                if let Some(tx) = self.workspace_tx.as_mut() {
                    let _ = tx.try_send(WorkspaceEvent::Activate(id));
                }
            }
            Message::WheelScrolled(delta) => {
                let discrete_delta = self.scroll.update(delta);
                if discrete_delta.y != 0
                    && let Some(w_i) = self
                        .workspaces
                        .iter()
                        .position(|w| w.state.contains(ext_workspace_handle_v1::State::Active))
                {
                    let d_i = (w_i as isize - discrete_delta.y)
                        .rem_euclid(self.workspaces.len() as isize)
                        as usize;

                    if let Some(tx) = self.workspace_tx.as_mut() {
                        let _ = tx.try_send(WorkspaceEvent::Activate(
                            self.workspaces[d_i].handle.clone(),
                        ));
                    }
                }
            }
            Message::WorkspaceOverview => {
                let _ = ShellCommand::new("cosmic-workspaces").spawn();
            }
            Message::Surface(a) => {
                return cosmic::task::message(cosmic::Action::Cosmic(
                    cosmic::app::Action::Surface(a),
                ));
            }
        }
        Task::none()
    }

    fn view(&self) -> Element<'_, Message> {
        if self.workspaces.is_empty() {
            return row![].padding(8).into();
        }
        let suggested_total = self.suggested_button_size();
        let suggested_window_size = self.core.applet.suggested_window_size();
        let popup_index = self.popup_index().unwrap_or(self.workspaces.len());

        let buttons = self.workspaces[..popup_index].iter().map(|w| {
            let horizontal = self.core.applet.is_horizontal();
            let apps = self.apps_for_workspace(w);
            let (width, height) = if horizontal {
                (
                    self.workspace_button_major_size(w),
                    suggested_window_size.1.get() as f32,
                )
            } else {
                (suggested_window_size.0.get() as f32, suggested_total)
            };

            let tooltip = self.workspace_tooltip(w, &apps);
            let visible_app_count = if horizontal {
                apps.len().min(MAX_VISIBLE_APPS)
            } else {
                apps.len().min(2)
            };
            let icon_size = if horizontal {
                self.app_icon_size()
            } else {
                (width.min(height) * 0.3).clamp(10.0, 14.0)
            };
            let icons = apps
                .iter()
                .take(visible_app_count)
                .map(|app| self.app_icon(app.metadata, icon_size, app.all_minimized()));
            let mut app_strip = row(icons)
                .spacing(if horizontal { APP_ICON_SPACING } else { 1.0 })
                .align_y(Alignment::Center);
            if apps.len() > visible_app_count {
                let overflow = if horizontal {
                    format!("+{}", apps.len() - visible_app_count)
                } else {
                    "…".to_owned()
                };
                app_strip = app_strip.push(
                    self.core
                        .applet
                        .text(overflow)
                        .size((icon_size * 0.55).max(10.0)),
                );
            }

            let number: Element<'_, Message> =
                container(self.core.applet.text(&w.name).font(cosmic::font::bold()))
                    .width(Length::Fixed(self.number_section_width(!apps.is_empty())))
                    .align_x(Alignment::Center)
                    .align_y(Alignment::Center)
                    .into();

            let content: Element<'_, Message> = if apps.is_empty() {
                number
            } else {
                let app_group: Element<'_, Message> = container(app_strip)
                    .padding(if horizontal {
                        [APP_GROUP_VERTICAL_PADDING, APP_GROUP_HORIZONTAL_PADDING]
                    } else {
                        [1.0, 3.0]
                    })
                    .into();

                if horizontal {
                    let divider: Element<'_, Message> =
                        container(space::vertical().height(Length::Fixed(icon_size * 0.8)))
                            .width(Length::Fixed(WORKSPACE_DIVIDER_WIDTH))
                            .class(ContainerClass::Custom(Box::new(|theme| container::Style {
                                background: Some(Background::Color(
                                    theme.current_container().divider.into(),
                                )),
                                ..Default::default()
                            })))
                            .into();

                    row![number, divider, app_group]
                        .spacing(WORKSPACE_CONTENT_SPACING)
                        .align_y(Alignment::Center)
                        .into()
                } else {
                    column![number, app_group]
                        .spacing(0)
                        .align_x(Alignment::Center)
                        .into()
                }
            };

            let btn = button(
                container(content)
                    .width(Length::Fixed(width))
                    .height(Length::Fixed(height))
                    .padding(if horizontal && !apps.is_empty() {
                        Padding {
                            left: WORKSPACE_LEADING_PADDING,
                            right: WORKSPACE_TRAILING_PADDING,
                            ..Padding::ZERO
                        }
                    } else {
                        Padding::ZERO
                    })
                    .align_x(Alignment::Center)
                    .align_y(Alignment::Center),
            )
            .on_press(
                if w.state.contains(ext_workspace_handle_v1::State::Active) {
                    Message::WorkspaceOverview
                } else {
                    Message::WorkspacePressed(w.handle.clone())
                },
            )
            .padding(0);

            let has_apps = !apps.is_empty();
            let btn = btn.class(
                if w.state.contains(ext_workspace_handle_v1::State::Active) {
                    cosmic::theme::iced::Button::Primary
                } else if w.state.contains(ext_workspace_handle_v1::State::Urgent) {
                    let appearance = |theme: &Theme| {
                        let cosmic = theme.cosmic();
                        button::Style {
                            background: Some(Background::Color(cosmic.palette.neutral_3.into())),
                            border: Border {
                                radius: cosmic.radius_xl().into(),
                                ..Default::default()
                            },
                            border_radius: theme.cosmic().radius_xl().into(),
                            text_color: theme.cosmic().destructive_button.base.into(),
                            ..button::Style::default()
                        }
                    };
                    cosmic::theme::iced::Button::Custom(Box::new(
                        move |theme, status| match status {
                            button::Status::Active => appearance(theme),
                            button::Status::Hovered => button::Style {
                                background: Some(Background::Color(
                                    theme.current_container().component.hover.into(),
                                )),
                                border: Border {
                                    radius: theme.cosmic().radius_xl().into(),
                                    ..Default::default()
                                },
                                ..appearance(theme)
                            },
                            button::Status::Pressed => appearance(theme),
                            button::Status::Disabled => appearance(theme),
                        },
                    ))
                } else {
                    let appearance = move |theme: &Theme| {
                        let cosmic = theme.cosmic();
                        button::Style {
                            background: has_apps.then(|| {
                                Background::Color(theme.current_container().small_widget.into())
                            }),
                            border: Border {
                                color: theme.current_container().divider.into(),
                                width: if has_apps { 1.0 } else { 0.0 },
                                radius: cosmic.radius_xl().into(),
                            },
                            border_radius: cosmic.radius_xl().into(),
                            text_color: theme.current_container().component.on.into(),
                            ..button::Style::default()
                        }
                    };
                    cosmic::theme::iced::Button::Custom(Box::new(
                        move |theme, status| match status {
                            button::Status::Active => appearance(theme),
                            button::Status::Hovered => button::Style {
                                background: Some(Background::Color(
                                    theme.current_container().component.hover.into(),
                                )),
                                border: Border {
                                    radius: theme.cosmic().radius_xl().into(),
                                    ..Default::default()
                                },
                                ..appearance(theme)
                            },
                            button::Status::Pressed | button::Status::Disabled => appearance(theme),
                        },
                    ))
                },
            );

            self.core
                .applet
                .applet_tooltip(btn, tooltip, false, Message::Surface, None)
                .into()
        });
        // TODO if there is a popup_index, create a button with a popup for the remaining workspaces
        // Should it appear on hover or on click?
        let layout_section: Element<_> = match self.layout {
            Layout::Row => row(buttons).spacing(4).into(),
            Layout::Column => column(buttons).spacing(4).into(),
        };
        let mut limits = Limits::NONE.min_width(1.).min_height(1.);
        if let Some(b) = self.core.applet.suggested_bounds {
            if b.width as i32 > 0 {
                limits = limits.max_width(b.width);
            }
            if b.height as i32 > 0 {
                limits = limits.max_height(b.height);
            }
        }

        autosize::autosize(
            container(layout_section).padding(0),
            AUTOSIZE_MAIN_ID.clone(),
        )
        .limits(limits)
        .into()
    }

    fn subscription(&self) -> Subscription<Message> {
        Subscription::batch([
            workspaces().map(Message::WorkspaceUpdate),
            event::listen_with(|e, _, _| match e {
                Mouse(mouse::Event::WheelScrolled { delta }) => Some(Message::WheelScrolled(delta)),
                _ => None,
            }),
        ])
    }

    fn style(&self) -> Option<cosmic::iced::theme::Style> {
        Some(cosmic::applet::style())
    }
}
