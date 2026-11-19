#![allow(non_snake_case)]

use gpui::{
    App, AppContext, Context, EventEmitter, FocusHandle, Focusable, InteractiveElement,
    IntoElement, ParentElement, Render, SharedString, Styled, Window, actions, div,
};
use workspace::{Item, Workspace};

actions!(
    wyrmcoil,
    [
        /// Open the WyrmCoil Preview tab.
        OpenPreview
    ]
);

pub fn Init(cx: &mut App) {
    cx.observe_new(|workspace: &mut Workspace, _, _| {
        workspace.register_action(|workspace, _: &OpenPreview, window, cx| {
            if let Some(preview) = workspace.item_of_type::<Preview>(cx) {
                workspace.activate_item(&preview, true, true, window, cx);
                return;
            }

            let preview = cx.new(|cx| Preview {
                focus: cx.focus_handle(),
                title: format!("{} Preview", wyrmcoil::ProjectName()).into(),
            });
            workspace.add_item_to_active_pane(Box::new(preview), None, true, window, cx);
        });
    })
    .detach();
}

struct Preview {
    focus: FocusHandle,
    title: SharedString,
}

impl Focusable for Preview {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for Preview {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .track_focus(&self.focus)
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .child(self.title.clone())
    }
}

impl EventEmitter<()> for Preview {}

impl Item for Preview {
    type Event = ();

    fn tab_content_text(&self, _: usize, _: &App) -> SharedString {
        self.title.clone()
    }
}
