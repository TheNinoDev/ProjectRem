// Included from app.rs - the page renderers.
//
// Every function here takes plain data rather than `&mut ProjectRemApp`, which
// keeps the UI thread free of aliasing problems and makes the pages easy to
// reason about: render, report at most one action, done.

fn view_main(ui: &mut Ui, data: &MainView<'_>) {
    scroll(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.add_space(8.0);

        widgets::card(ui, |ui| {
            ui.label(RichText::new(data.title).size(17.0).strong().color(theme::TEXT_STRONG));
            ui.add_space(6.0);
            ui.label(RichText::new(data.body).size(14.0).color(theme::TEXT_NORMAL));
            ui.add_space(10.0);
            ui.label(RichText::new(&data.stamp).size(12.0).color(theme::TEXT_FAINT));
        });

        ui.add_space(16.0);
        ui.label(RichText::new(data.status_label).size(13.0).strong().color(theme::TEXT_STRONG));
        ui.add_space(8.0);
        for (label, value, ok) in &data.rows {
            widgets::status_row(ui, label, value, *ok);
        }
        ui.add_space(12.0);
    });
}

/// Returns the index of the model whose Download / Cancel button was pressed.
fn view_models(ui: &mut Ui, data: &ModelsView) -> Option<usize> {
    let mut pressed = None;
    scroll(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.add_space(8.0);
        for (index, (model, state, missing)) in data.models.iter().enumerate() {
            if model_card(ui, model, state, *missing) {
                pressed = Some(index);
            }
        }
        ui.add_space(16.0);
    });
    pressed
}

/// Returns 	rue when the Download / Cancel button was clicked.
fn model_card(ui: &mut Ui, model: &ModelDef, state: &DownloadState, missing: usize) -> bool {
    let i18n = I18n::new(Language::English);
    let t: Tr = I18n::t;
    let active = state.is_active();
    let mut clicked = false;
    let (label, icon) = if active {
        (t(i18n, Msg::Cancel), Icon::X)
    } else {
        (t(i18n, Msg::Download), Icon::Download)
    };

    widgets::card(ui, |ui| {
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(&model.name).size(15.0).strong().color(theme::TEXT_STRONG));
                    ui.add_space(4.0);
                    widgets::pill(ui, model.kind.label(), theme::ACCENT);
                });
                ui.label(RichText::new(&model.description).size(13.0).color(theme::TEXT_MUTED));
            });

            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if active {
                    if widgets::secondary_button(ui, icon, label).clicked() {
                        clicked = true;
                    }
                } else if widgets::primary_button(ui, icon, label).clicked() {
                    clicked = true;
                }
            });
        });

        ui.add_space(10.0);
        let meta = format!(
            "{} {}   ·   {} {}   ·   {}",
            t(i18n, Msg::Size),
            paths::human_bytes(model.approx_bytes),
            t(i18n, Msg::Files),
            model.source.files().len(),
            model.source.label(),
        );
        ui.label(RichText::new(meta).size(12.0).color(theme::TEXT_FAINT));

        ui.add_space(6.0);
        let _ = ui.hyperlink_to(model.repo_url.clone(), model.repo_url.clone());

        ui.add_space(10.0);
        model_status(ui, model, state, missing);
    });
    clicked
}

fn model_status(ui: &mut Ui, model: &ModelDef, state: &DownloadState, missing: usize) {
    let i18n = I18n::new(Language::English);
    let t: Tr = I18n::t;
    match state {
        DownloadState::Missing => {
            let text =
                format!("{} · {missing} {}", t(i18n, Msg::NotDownloaded), t(i18n, Msg::Files));
            ui.label(RichText::new(text).size(12.5).color(theme::TEXT_MUTED));
        }
        DownloadState::Queued { position } => {
            let text = format!("{} #{position}", t(i18n, Msg::Queued));
            ui.label(RichText::new(text).size(12.5).color(theme::WARNING));
        }
        DownloadState::Running { bytes_done, bytes_total, current_file, started } => {
            ui.add(
                egui::ProgressBar::new(state.fraction().unwrap_or(0.0))
                    .fill(theme::ACCENT)
                    .desired_height(6.0)
                    .corner_radius(CornerRadius::same(3)),
            );
            ui.add_space(6.0);
            let elapsed = started.elapsed().as_secs();
            let right = if *bytes_total > 0 {
                format!(
                    "{} / {}   {}",
                    paths::human_bytes(*bytes_done),
                    paths::human_bytes(*bytes_total),
                    paths::human_duration(elapsed)
                )
            } else {
                format!("{}   {}", paths::human_bytes(*bytes_done), paths::human_duration(elapsed))
            };
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!("{} · {current_file}", t(i18n, Msg::Downloading)))
                        .size(12.0)
                        .color(theme::TEXT_MUTED),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(RichText::new(right).size(12.0).color(theme::TEXT_FAINT));
                });
            });
        }
        DownloadState::Done => {
            let size = model.source.size_on_disk(&model.id);
            let text = format!("{} · {}", t(i18n, Msg::Ready), paths::human_bytes(size));
            ui.label(RichText::new(text).size(12.5).color(theme::SUCCESS));
        }
        DownloadState::Cancelled => {
            ui.label(RichText::new(t(i18n, Msg::Cancelled)).size(12.5).color(theme::WARNING));
        }
        DownloadState::Failed(error) => {
            let text = format!("{}: {error}", t(i18n, Msg::Failed));
            ui.label(RichText::new(text).size(12.5).color(theme::DANGER));
        }
    }
}

/// What the Account page asks the shell to do.
enum AccountPage {
    Nothing,
    /// Login or Register with whatever is in the form.
    Submit,
    SwitchMode(AuthMode),
    Action(AccountAction),
}

fn view_account(
    ui: &mut Ui,
    data: &AccountView,
    avatars: &mut widgets::AvatarCache,
    username: &mut String,
    password: &mut String,
    show_password: &mut bool,
    hf_token: &mut String,
) -> AccountPage {
    if data.signed_in {
        let mut action = None;
        profile(ui, data, avatars, hf_token, &mut action);
        match action {
            Some(action) => AccountPage::Action(action),
            None => AccountPage::Nothing,
        }
    } else {
        auth_form(ui, data, username, password, show_password)
    }
}

/// Width of the sign-in card's contents.
const AUTH_WIDTH: f32 = 380.0;

fn auth_form(
    ui: &mut Ui,
    data: &AccountView,
    username: &mut String,
    password: &mut String,
    show_password: &mut bool,
) -> AccountPage {
    let registering = data.mode == AuthMode::Register;
    let mut submit = false;
    let mut toggle: Option<AuthMode> = None;
    let mut toggle_password = false;

    // Centre a fixed-width column in the page. The rect is built from the
    // *available* area rather than the cursor, so a page that already spent
    // vertical space does not shift the column sideways.
    let available = ui.available_rect_before_wrap();
    let origin = egui::pos2(
        available.min.x + (available.width() - AUTH_WIDTH).max(0.0) / 2.0,
        available.min.y + 24.0,
    );
    let height = (available.max.y - origin.y).max(0.0);
    let mut column = ui.new_child(
        egui::UiBuilder::new()
            .id_salt("auth")
            .max_rect(egui::Rect::from_min_size(origin, egui::vec2(AUTH_WIDTH, height)))
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    let ui = &mut column;

    {
        widgets::card(ui, |ui| {
            ui.horizontal(|ui| {
                if widgets::segment(ui, &data.sign_in, !registering).clicked() {
                    toggle = Some(AuthMode::Login);
                }
                if widgets::segment(ui, &data.register, registering).clicked() {
                    toggle = Some(AuthMode::Register);
                }
            });
            ui.add_space(14.0);

            let heading = if registering { &data.create_account } else { &data.sign_in };
            let subtitle = if registering { &data.register_subtitle } else { &data.sign_in_subtitle };
            ui.label(RichText::new(heading).size(17.0).strong().color(theme::TEXT_STRONG));
            ui.add_space(4.0);
            ui.label(RichText::new(subtitle).size(12.5).color(theme::TEXT_FAINT));

            ui.add_space(16.0);
            ui.label(RichText::new(&data.username_label).size(12.5).color(theme::TEXT_MUTED));
            ui.add_space(4.0);
            ui.add(
                egui::TextEdit::singleline(username)
                    .desired_width(f32::INFINITY)
                    .hint_text("nikla")
                    .font(egui::TextStyle::Body),
            );

            ui.add_space(12.0);
            ui.label(RichText::new(&data.password_label).size(12.5).color(theme::TEXT_MUTED));
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                // The eye button sits next to the field, so the field takes the
                // rest of the row rather than the full card width.
                ui.add(
                    egui::TextEdit::singleline(password)
                        .desired_width(
                            (AUTH_WIDTH - 2.0 * 16.0 - widgets::ICON_BUTTON_WIDTH - 8.0).max(120.0),
                        )
                        .hint_text("••••••••")
                        .font(egui::TextStyle::Body)
                        .password(!*show_password),
                );
                if widgets::icon_button(
                    ui,
                    if *show_password { Icon::EyeOff } else { Icon::Eye },
                    &data.show_password_label,
                    false,
                )
                .clicked()
                {
                    toggle_password = true;
                }
            });

            ui.add_space(18.0);
            let icon = if registering { Icon::UserPlus } else { Icon::LogIn };
            let label = if data.busy { data.working_label.as_str() } else { heading.as_str() };
            ui.horizontal(|ui| {
                // Centre the button by padding, so it stays centred whatever
                // its label measures.
                let width = widgets::button_width(ui, icon, label);
                ui.add_space(((AUTH_WIDTH - 2.0 * 16.0) - width).max(0.0) / 2.0);
                if data.busy {
                    widgets::disabled_button(ui, icon, label);
                } else if widgets::primary_button(ui, icon, label).clicked() {
                    submit = true;
                }
            });
        });

        // The server line sits directly under the card, centred by padding
        // rather than by a centred layout - the column is already positioned,
        // and a nested centred layout would re-centre it against the page.
        ui.add_space(14.0);
        ui.horizontal(|ui| {
            ui.add_space(
                ((AUTH_WIDTH - widgets::text_width(ui, &data.account_server_label)).max(0.0)) / 2.0,
            );
            ui.label(
                RichText::new(&data.account_server_label).size(11.5).color(theme::TEXT_FAINT),
            );
        });
        ui.add_space(3.0);
        let url = egui::RichText::new(&data.server).size(11.5).color(theme::ACCENT);
        ui.horizontal(|ui| {
            ui.add_space(((AUTH_WIDTH - widgets::text_width(ui, &data.server)).max(0.0)) / 2.0);
            if ui.add(egui::Label::new(url).sense(egui::Sense::click())).clicked() {
                ui.ctx().open_url(egui::OpenUrl::same_tab(&data.server));
            }
        });
    }

    if toggle_password {
        *show_password = !*show_password;
    }
    if submit && !username.is_empty() && !password.is_empty() {
        return AccountPage::Submit;
    }
    match toggle {
        Some(mode) => AccountPage::SwitchMode(mode),
        None => AccountPage::Nothing,
    }
}

fn profile(
    ui: &mut Ui,
    data: &AccountView,
    avatars: &mut widgets::AvatarCache,
    hf_token: &mut String,
    action: &mut Option<AccountAction>,
) {
    scroll(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.add_space(8.0);

        // Header -----------------------------------------------------------
        widgets::card(ui, |ui| {
            ui.horizontal(|ui| {
                let (rect, _) = ui.allocate_exact_size(Vec2::splat(64.0), Sense::hover());
                match &data.user.avatar_url {
                    Some(url) => widgets::avatar_image(ui, avatars, url, rect),
                    None => widgets::fallback_avatar(ui, rect, theme::ACCENT),
                }
                ui.vertical(|ui| {
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new(&data.user.display_name)
                            .size(19.0)
                            .strong()
                            .color(theme::TEXT_STRONG),
                    );
                    ui.label(
                        RichText::new(format!("@{}", data.user.username))
                            .size(13.0)
                            .color(theme::TEXT_MUTED),
                    );
                    let day: String = data.user.created_at.chars().take(10).collect();
                    if !day.is_empty() {
                        ui.label(
                            RichText::new(format!("{} {day}", data.member_since))
                                .size(12.0)
                                .color(theme::TEXT_FAINT),
                        );
                    }
                });
            });
        });

        // Profile picture ---------------------------------------------------
        ui.add_space(16.0);
        widgets::section(ui, Icon::Image, &data.profile_picture);
        ui.add_space(8.0);
        widgets::card(ui, |ui| {
            ui.label(RichText::new(&data.profile_picture_hint).size(12.5).color(theme::TEXT_FAINT));
            ui.add_space(12.0);
            if data.avatar_choices.is_empty() {
                ui.label(RichText::new(&data.no_avatars).size(13.0).color(theme::TEXT_MUTED));
            } else {
                ui.horizontal_wrapped(|ui| {
                    for (provider, name, url) in &data.avatar_choices {
                        let selected = data.user.avatar_url.as_deref() == Some(url.as_str());
                        if widgets::avatar_choice(ui, avatars, *provider, name, url, selected).clicked()
                        {
                            *action = Some(AccountAction::PickAvatar(Some(url.clone())));
                        }
                    }
                    if widgets::avatar_choice_none(ui, &data.no_picture, data.user.avatar_url.is_none())
                        .clicked()
                    {
                        *action = Some(AccountAction::PickAvatar(None));
                    }
                });
            }
        });

        // Connections -------------------------------------------------------
        ui.add_space(16.0);
        widgets::section(ui, Icon::Link, &data.connections_label);
        ui.add_space(8.0);
        widgets::card(ui, |ui| {
            ui.label(RichText::new(&data.connections_hint).size(12.5).color(theme::TEXT_FAINT));
            ui.add_space(12.0);
            for provider in Provider::ALL {
                let connection = data.connections.iter().find(|c| c.provider == provider);
                let detail = connection
                    .and_then(|c| c.username.clone())
                    .unwrap_or_else(|| data.not_connected.clone());
                let row = widgets::connection_row(
                    ui,
                    provider,
                    &detail,
                    connection.is_some(),
                    data.oauth_busy,
                    &data.disconnect,
                    &data.connect,
                );
                match row {
                    Some(widgets::RowClick::Connect(p)) => *action = Some(AccountAction::Connect(p)),
                    Some(widgets::RowClick::Disconnect(p)) => {
                        *action = Some(AccountAction::Disconnect(p))
                    }
                    None => {}
                }
            }
        });

        // Hugging Face token ------------------------------------------------
        ui.add_space(16.0);
        widgets::section(ui, Icon::KeyRound, &data.hf_token);
        ui.add_space(8.0);
        let stored_label = match (&data.hf_token_stored, &data.hf_token_stored_at) {
            (true, Some(at)) => data.token_stored_at.replace("{date}", at),
            (true, None) => data.token_stored.clone(),
            _ => data.token_not_stored.clone(),
        };
        widgets::card(ui, |ui| {
            ui.label(RichText::new(&data.hf_token_hint).size(12.5).color(theme::TEXT_FAINT));
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(hf_token)
                        .desired_width(f32::INFINITY)
                        .hint_text("hf_...")
                        .font(egui::TextStyle::Body)
                        .password(true),
                );
                if data.busy {
                    widgets::disabled_button(ui, Icon::Save, "");
                } else if widgets::primary_button(ui, Icon::Save, &data.save_token).clicked() {
                    *action = Some(AccountAction::SaveHfToken(hf_token.clone()));
                }
            });
            ui.add_space(8.0);
            ui.label(
                RichText::new(stored_label)
                    .size(12.0)
                    .color(if data.hf_token_stored { theme::SUCCESS } else { theme::TEXT_FAINT }),
            );
        });

        // Session -----------------------------------------------------------
        ui.add_space(16.0);
        widgets::card(ui, |ui| {
            if data.busy {
                widgets::disabled_button(ui, Icon::LogOut, &data.sign_out);
            } else if widgets::danger_button(ui, Icon::LogOut, &data.sign_out).clicked() {
                *action = Some(AccountAction::SignOut);
            }
        });
        ui.add_space(16.0);
    });
}

fn view_settings(
    ui: &mut Ui,
    data: &SettingsView,
    settings: &mut Settings,
    action: &mut Option<ToolbarAction>,
) {
    let mut input = data.input_device.clone();
    let mut output = data.output_device.clone();
    let mut language = data.software_language;

    scroll(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.add_space(8.0);

        widgets::section(ui, Icon::Volume2, &data.audio);
        ui.add_space(8.0);
        widgets::card(ui, |ui| {
            ui.label(RichText::new(&data.audio_hint).size(12.5).color(theme::TEXT_FAINT));
            ui.add_space(12.0);

            ui.label(RichText::new(&data.input_label).size(13.0).color(theme::TEXT_NORMAL));
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                device_combo(
                    ui,
                    "input_device",
                    &mut input,
                    &data.devices.inputs,
                    &data.system_default,
                    &data.no_devices,
                );
                if widgets::icon_button(ui, Icon::RefreshCw, &data.refresh, false).clicked() {
                    *action = Some(ToolbarAction::RefreshDevices);
                }
            });

            ui.add_space(12.0);
            ui.label(RichText::new(&data.output_label).size(13.0).color(theme::TEXT_NORMAL));
            ui.add_space(4.0);
            device_combo(
                ui,
                "output_device",
                &mut output,
                &data.devices.outputs,
                &data.system_default,
                &data.no_devices,
            );
        });

        ui.add_space(16.0);
        widgets::section(ui, Icon::Languages, &data.language);
        ui.add_space(8.0);
        widgets::card(ui, |ui| {
            ui.label(RichText::new(&data.language_hint).size(12.5).color(theme::TEXT_FAINT));
            ui.add_space(10.0);
            language_combo(ui, "software_language", &mut language);
        });

        ui.add_space(16.0);
        widgets::section(ui, Icon::Info, &data.about);
        ui.add_space(8.0);
        widgets::card(ui, |ui| {
            widgets::key_value(ui, &data.version_label, version::VERSION);
            widgets::key_value(ui, &data.build_label, &data.build);
            widgets::key_value(ui, &data.backend_label, &data.api);
            widgets::key_value(ui, &data.data_folder_label, &data.data_dir);
            ui.add_space(10.0);
            let _ = ui.hyperlink_to(data.api.clone(), data.api.clone());
        });
        ui.add_space(16.0);
    });

    let mut dirty = false;
    if input != settings.input_device {
        settings.input_device = input;
        dirty = true;
    }
    if output != settings.output_device {
        settings.output_device = output;
        dirty = true;
    }
    if language != settings.software_language {
        settings.software_language = language;
        dirty = true;
    }
    if dirty {
        let _ = settings.save();
    }
}

fn view_plugins(ui: &mut Ui, data: &PluginsView) -> Option<PluginsAction> {
    let mut action = None;
    scroll(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.add_space(8.0);
        ui.label(RichText::new(&data.hint).size(12.5).color(theme::TEXT_FAINT));
        ui.add_space(12.0);

        if data.plugins.is_empty() {
            ui.label(RichText::new(&data.empty).size(13.0).color(theme::TEXT_MUTED));
        }

        for (index, plugin) in data.plugins.iter().enumerate() {
            let manifest = plugin.manifest.clone();
            let imported = plugin.origin == Origin::Imported;
            let mut enabled = plugin.enabled;
            // Reserve the controls up front. Giving the text column all of the
            // remaining width instead pushes the switch off the card edge.
            let controls = widgets::SWITCH_WIDTH
                + if imported { widgets::ICON_BUTTON_WIDTH + 8.0 } else { 0.0 }
                + 8.0;

            widgets::card(ui, |ui| {
                ui.horizontal(|ui| {
                    let (icon_rect, _) = ui.allocate_exact_size(Vec2::new(34.0, 34.0), Sense::hover());
                    crate::lucide::paint(
                        ui.painter(),
                        Icon::Puzzle,
                        Rect::from_center_size(icon_rect.center(), Vec2::splat(22.0)),
                        theme::TEXT_MUTED,
                        2.0,
                    );
                    ui.vertical(|ui| {
                        ui.set_width((ui.available_width() - controls).max(120.0));
                        ui.label(
                            RichText::new(&manifest.name)
                                .size(15.0)
                                .strong()
                                .color(theme::TEXT_STRONG),
                        );
                        ui.label(
                            RichText::new(format!("v{}  ·  {}", manifest.version, manifest.author))
                                .size(12.0)
                                .color(theme::TEXT_FAINT),
                        );
                        ui.label(
                            RichText::new(&manifest.description)
                                .size(13.0)
                                .color(theme::TEXT_NORMAL),
                        );
                    });
                    ui.horizontal(|ui| {
                        if imported {
                            if widgets::icon_button(ui, Icon::Trash2, &data.remove, true).clicked() {
                                action = Some(PluginsAction::Remove(index));
                            }
                        }
                        if widgets::switch(ui, &mut enabled).clicked() {
                            action = Some(PluginsAction::Toggle(index, !enabled));
                        }
                    });
                });
            });
        }
        ui.add_space(16.0);
    });
    action
}

fn language_combo(ui: &mut Ui, id: &'static str, current: &mut Language) {
    egui::ComboBox::from_id_salt(id)
        .selected_text(format!("{}  ({})", current.native_name(), current.flag()))
        .width(260.0)
        .show_ui(ui, |ui| {
            for language in Language::ALL {
                ui.selectable_value(
                    current,
                    language,
                    format!("{}  ({})", language.native_name(), language.flag()),
                );
            }
        });
}

fn device_combo(
    ui: &mut Ui,
    id: &'static str,
    current: &mut String,
    devices: &[DeviceInfo],
    default_label: &str,
    none_label: &str,
) {
    let label = if current == SYSTEM_DEFAULT {
        default_label.to_owned()
    } else {
        devices
            .iter()
            .find(|d| d.id == *current)
            .map(|d| d.name.clone())
            .unwrap_or_else(|| current.clone())
    };
    egui::ComboBox::from_id_salt(id)
        .selected_text(label)
        .width(300.0)
        .show_ui(ui, |ui| {
            ui.selectable_value(current, SYSTEM_DEFAULT.to_owned(), default_label.to_owned());
            for device in devices {
                ui.selectable_value(current, device.id.clone(), device.name.clone());
            }
            if devices.is_empty() {
                ui.add_enabled(false, egui::Button::new(none_label.to_owned()));
            }
        });
}