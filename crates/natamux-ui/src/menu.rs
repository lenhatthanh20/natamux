//! The actions of an open menu. The app sends them by name (`menu::SelectNext`), so the
//! namespace stays `menu` (plan 6.7.1, D22).

use gpui::actions;

actions!(
    menu,
    [
        /// Closes the menu.
        Cancel,
        /// Runs the picked row.
        Confirm,
        /// Picks the row above.
        SelectPrevious,
        /// Picks the row below.
        SelectNext,
        /// Picks the first row.
        SelectFirst,
        /// Picks the last row.
        SelectLast,
        /// Opens the submenu of the picked row.
        SelectChild,
        /// Goes back to the menu that opened this one.
        SelectParent,
    ]
);

#[cfg(test)]
mod tests {
    use gpui::Action;

    use super::*;

    #[test]
    fn the_actions_keep_the_names_the_app_sends() {
        let names = [
            Cancel::name_for_type(),
            Confirm::name_for_type(),
            SelectPrevious::name_for_type(),
            SelectNext::name_for_type(),
            SelectFirst::name_for_type(),
            SelectLast::name_for_type(),
            SelectChild::name_for_type(),
            SelectParent::name_for_type(),
        ];
        assert_eq!(
            names,
            [
                "menu::Cancel",
                "menu::Confirm",
                "menu::SelectPrevious",
                "menu::SelectNext",
                "menu::SelectFirst",
                "menu::SelectLast",
                "menu::SelectChild",
                "menu::SelectParent",
            ]
        );
    }
}
