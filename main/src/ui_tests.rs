//! Headless UI tests.
//!
//! egui runs without a display: [`Context::run_ui`] takes a [`RawInput`] and
//! runs one frame, returning the full output. That makes it possible to assert
//! on behaviour - "a click on the Models row switches the page" - instead of on
//! pixels, which is exactly what broke: the sidebar looked perfect and did
//! nothing.

use egui::{Context, Pos2, RawInput, Rect, Vec2};

use super::app::{ProjectRemApp, Tab};
use super::theme;

/// A 1360x860 frame with the pointer at `pos`.
fn frame(pos: Pos2) -> RawInput {
    let mut raw = RawInput::default();
    raw.screen_rect = Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1360.0, 860.0)));
    raw.events.push(egui::Event::PointerMoved(pos));
    raw
}

/// Run one frame and drop the output safely.
///
/// A real renderer uploads `TexturesDelta` to the GPU; with no GPU here the
/// deltas have to be cleared explicitly or epaint asserts on drop. Font
/// atlases are uploaded this way, so it fires on the very first frame.
fn run(ctx: &Context, app: &mut ProjectRemApp, raw: RawInput) -> egui::FullOutput {
    let mut output = ctx.run_ui(raw, |ui| app.draw_frame(ui));
    output.textures_delta.clear();
    output
}

/// Press and release the primary button at `pos`.
fn click(ctx: &Context, app: &mut ProjectRemApp, pos: Pos2) {
    let mut raw = frame(pos);
    for pressed in [true, false] {
        raw.events.push(egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::default(),
        });
    }
    run(ctx, app, raw);
}

/// Lay the app out once so the nav rects are recorded.
fn settle(ctx: &Context, app: &mut ProjectRemApp) {
    run(ctx, app, frame(Pos2::new(-80.0, -80.0)));
}

fn app() -> (Context, ProjectRemApp) {
    let ctx = Context::default();
    let app = ProjectRemApp::for_test(&ctx);
    (ctx, app)
}

/// The centre of a sidebar nav row, as recorded by `sidebar()` during layout.
fn nav_centre(app: &ProjectRemApp, tab: Tab) -> Pos2 {
    app.nav_rect(tab)
        .unwrap_or_else(|| panic!("the {tab:?} nav row was never laid out"))
        .center()
}

#[test]
fn clicking_a_nav_row_switches_the_page() {
    for target in Tab::ALL {
        let (ctx, mut app) = app();
        settle(&ctx, &mut app);

        // Start on a different page, so a click cannot pass by accident.
        let elsewhere = Tab::ALL.into_iter().find(|t| *t != target).expect("two or more tabs");
        app.set_tab_for_test(elsewhere);
        assert_ne!(app.tab_for_test(), target);

        let pos = nav_centre(&app, target);
        click(&ctx, &mut app, pos);

        assert_eq!(
            app.tab_for_test(),
            target,
            "a click on the {target:?} nav row did not switch the page"
        );
    }
}

#[test]
fn the_page_body_never_overlaps_the_sidebar() {
    // The bug behind "I can't click the tabs": the page body claimed the full
    // window width, so it sat on top of the sidebar and swallowed its clicks.
    for tab in Tab::ALL {
        let (ctx, mut app) = app();
        settle(&ctx, &mut app);
        app.set_tab_for_test(tab);

        run(&ctx, &mut app, frame(Pos2::ZERO));
        let body = app.page_rect_for_test();

        assert!(
            body.left() >= theme::SIDEBAR_WIDTH - 0.5,
            "the {tab:?} page body starts at x={} and overlaps the {}-px sidebar",
            body.left(),
            theme::SIDEBAR_WIDTH
        );
    }
}

#[test]
fn nav_rows_are_stacked_top_to_bottom_without_overlapping() {
    let (ctx, mut app) = app();
    settle(&ctx, &mut app);

    let mut previous: Option<Rect> = None;
    for tab in Tab::ALL {
        let rect = app.nav_rect(tab).expect("nav row was laid out");
        assert!(rect.width() > 100.0, "the {tab:?} nav row is too narrow: {rect:?}");
        assert!(rect.height() >= 30.0, "the {tab:?} nav row is too short: {rect:?}");
        if let Some(before) = previous {
            assert!(
                rect.top() >= before.bottom() - 0.5,
                "the {tab:?} nav row overlaps the one above it: {rect:?} vs {before:?}"
            );
        }
        previous = Some(rect);
    }
}

#[test]
fn the_pointer_reaches_every_nav_row() {
    for tab in Tab::ALL {
        let (ctx, mut app) = app();
        settle(&ctx, &mut app);

        let pos = nav_centre(&app, tab);
        run(&ctx, &mut app, frame(pos));
        assert!(
            app.nav_hovered_for_test(tab),
            "the pointer at {pos} does not reach the {tab:?} nav row"
        );
    }
}

#[test]
fn the_page_body_ends_above_the_action_bar() {
    // The action bar is a bottom panel; if the body ignored it, the bar's
    // buttons would be unreachable.
    for tab in [Tab::Models, Tab::Settings, Tab::Plugins] {
        let (ctx, mut app) = app();
        settle(&ctx, &mut app);
        app.set_tab_for_test(tab);

        run(&ctx, &mut app, frame(Pos2::ZERO));
        let body = app.page_rect_for_test();
        assert!(
            body.max.y <= 860.0 - theme::ACTION_BAR_HEIGHT + 0.5,
            "the {tab:?} page body runs to y={} and covers the action bar",
            body.max.y
        );
    }
}
