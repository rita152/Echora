use gpui::{VisualTestContext, point};

use super::{
    HomeView,
    landing::{HeroHeading, hero_heading},
};
use crate::{
    components::composer::WorkspacePresentation,
    git_review::Checkout,
    i18n::{Language, language, set_language},
    theme::ThemeMode,
};

struct RestoreLanguage(Language);
impl Drop for RestoreLanguage {
    fn drop(&mut self) {
        set_language(self.0);
    }
}

fn workspace(label: Option<&str>, checkout: Option<Checkout>) -> WorkspacePresentation {
    WorkspacePresentation {
        project_label: label.map(|label| label.to_owned().into()),
        checkout,
    }
}

fn heading(lead: &str, project: Option<&str>, tail: &str) -> Option<HeroHeading> {
    Some(HeroHeading {
        lead: lead.to_owned(),
        project: project.map(str::to_owned),
        tail: tail.to_owned(),
    })
}

#[test]
fn hero_heading_follows_the_reference_copy_for_each_workspace() {
    let _restore = RestoreLanguage(language());
    let repository = Some(Checkout::Branch("main".into()));
    let folder = Some(Checkout::NotRepository);
    let detached = Some(Checkout::Detached);

    set_language(Language::SimplifiedChinese);
    assert_eq!(
        hero_heading(&workspace(None, None)),
        heading("我们要构建什么？", None, "")
    );
    assert_eq!(
        hero_heading(&workspace(Some("LAG_创新"), repository.clone())),
        heading("你想让我们在 ", Some("LAG_创新"), " 中构建什么？")
    );
    assert_eq!(
        hero_heading(&workspace(Some("F题"), folder.clone())),
        heading("我们应该在", Some("F题"), "中做些什么？")
    );
    // A project waits for its checkout instead of flashing the wrong copy.
    assert_eq!(hero_heading(&workspace(Some("GPUI"), None)), None);

    set_language(Language::English);
    assert_eq!(
        hero_heading(&workspace(None, None)),
        heading("What should we build?", None, "")
    );
    // English keeps the question mark inside the underlined trigger.
    assert_eq!(
        hero_heading(&workspace(Some("LAG_创新"), repository)),
        heading("What should we build in ", Some("LAG_创新?"), "")
    );
    assert_eq!(
        hero_heading(&workspace(Some("GPUI"), detached)),
        heading("What should we build in ", Some("GPUI?"), "")
    );
    assert_eq!(
        hero_heading(&workspace(Some("F题"), folder)),
        heading("What should we work on in ", Some("F题?"), "")
    );
}

#[gpui::test]
fn empty_state_names_the_drafts_project_and_branch(cx: &mut gpui::TestAppContext) {
    let handle = cx.add_window(|_, cx| HomeView::new(ThemeMode::Dark, cx));
    let composer = handle
        .read_with(cx, |home, _| home.composer.clone())
        .unwrap();
    let mut visual = VisualTestContext::from_window(handle.into(), cx);
    let draw = |visual: &mut VisualTestContext| {
        visual.update(|window, cx| window.draw(cx).clear(cx));
        (
            visual.debug_bounds("home-hero-project"),
            visual.debug_bounds("composer-branch"),
            visual.debug_bounds("composer-clear-project"),
        )
    };

    // The host names the project; the checkout arrives from Git afterwards.
    // `set_workspace_context` is left out: a new cwd reloads the permission
    // catalog through the real app-server backend, outside the test scheduler.
    visual.update(|_, cx| {
        composer.update(cx, |composer, cx| {
            composer.set_project_label(Some("LAG_创新".into()), cx)
        })
    });
    let (hero, branch, clear) = draw(&mut visual);
    assert!(hero.is_none(), "the heading waits for the checkout");
    assert!(branch.is_none());
    assert!(clear.is_some(), "a project can be cleared");

    visual.update(|_, cx| {
        composer.update(cx, |composer, cx| {
            composer.set_checkout(
                Checkout::Branch("codex/s3-training-orchestration".into()),
                cx,
            )
        })
    });
    let (hero, branch, _) = draw(&mut visual);
    let hero = hero.expect("the project name is the heading's trigger");
    // One 33.6px heading line, rounded to the 1x test window's pixel grid.
    assert!((33.0..=34.0).contains(&f32::from(hero.size.height)));
    let branch = branch.expect("a branch checkout shows the branch control");
    // 8px padding, 16px icon, 4px gap, the value's 160px clamp, 8px padding.
    assert!((f32::from(branch.size.width) - 196.0).abs() <= 1.0);

    // Pointer over the trigger keeps the layout: only its colour changes.
    visual.simulate_mouse_move(
        point(hero.center().x, hero.center().y),
        None,
        gpui::Modifiers::default(),
    );
    let (hovered, _, _) = draw(&mut visual);
    assert_eq!(hovered, Some(hero));

    visual.update(|_, cx| {
        composer.update(cx, |composer, cx| {
            composer.set_checkout(Checkout::NotRepository, cx)
        })
    });
    let (hero, branch, _) = draw(&mut visual);
    assert!(hero.is_some(), "a plain folder is still named");
    assert!(branch.is_none(), "only a branch checkout shows the control");

    visual.update(|_, cx| composer.update(cx, |composer, cx| composer.set_project_label(None, cx)));
    let (hero, branch, clear) = draw(&mut visual);
    assert!(hero.is_none(), "a projectless draft names no project");
    assert!(branch.is_none());
    assert!(clear.is_none(), "there is no project to clear");
}
