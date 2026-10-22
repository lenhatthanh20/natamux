//! Which panels dock at a window width (`lib/layout-mode.ts`).

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SidebarMode {
    Docked,
    Drawer,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DetailsMode {
    Docked,
    Sheet,
    Page,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LayoutMode {
    pub sidebar: SidebarMode,
    pub details: DetailsMode,
    pub coarse: bool,
    /// Under 1024 a sheet or drawer comes with a scrim; above it the sheet floats without one.
    pub compact: bool,
}

/// The same steps as the Tailwind breakpoints md, lg and xl.
pub const BREAKPOINT_MD: f32 = 768.0;
pub const BREAKPOINT_LG: f32 = 1024.0;
pub const BREAKPOINT_XL: f32 = 1280.0;
pub const MIN_MAIN_WIDTH_FOR_DOCKED_DETAILS: f32 = 640.0;
/// One default for both side panels. At 1280 two of them still leave main its 640.
pub const PANEL_WIDTH: f32 = 320.0;

/// Without a measured width the window gets the desktop layout.
pub const DESKTOP_LAYOUT: LayoutMode = LayoutMode {
    sidebar: SidebarMode::Docked,
    details: DetailsMode::Docked,
    coarse: false,
    compact: false,
};

#[derive(Clone, Copy, Debug)]
pub struct LayoutInput {
    pub width: Option<f32>,
    /// The width the docked sidebar takes, 0 when the user collapsed it.
    pub sidebar_width: f32,
    pub details_width: f32,
    pub coarse: bool,
}

pub fn decide_layout(input: LayoutInput) -> LayoutMode {
    let Some(width) = input.width else {
        return LayoutMode {
            coarse: input.coarse,
            ..DESKTOP_LAYOUT
        };
    };
    let sidebar = if width >= BREAKPOINT_LG {
        SidebarMode::Docked
    } else {
        SidebarMode::Drawer
    };
    let mut details = if width >= BREAKPOINT_XL {
        DetailsMode::Docked
    } else if width >= BREAKPOINT_MD {
        DetailsMode::Sheet
    } else {
        DetailsMode::Page
    };
    // A wide sidebar must never crush the main area: then the details float over it.
    if details == DetailsMode::Docked {
        let sidebar_taken = if sidebar == SidebarMode::Docked {
            input.sidebar_width
        } else {
            0.0
        };
        if width - sidebar_taken - input.details_width < MIN_MAIN_WIDTH_FOR_DOCKED_DETAILS {
            details = DetailsMode::Sheet;
        }
    }
    LayoutMode {
        sidebar,
        details,
        coarse: input.coarse,
        compact: width < BREAKPOINT_LG,
    }
}

/// In 1024-1279 the sidebar may take at most 30% of the window.
pub fn effective_sidebar_width(stored: f32, width: Option<f32>) -> f32 {
    match width {
        Some(width) if (BREAKPOINT_LG..BREAKPOINT_XL).contains(&width) => {
            stored.min((width * 0.3).round())
        }
        _ => stored,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layout(width: Option<f32>, sidebar_width: f32, coarse: bool) -> LayoutMode {
        decide_layout(LayoutInput {
            width,
            sidebar_width,
            details_width: 360.0,
            coarse,
        })
    }

    fn base(width: f32) -> LayoutMode {
        layout(Some(width), 272.0, false)
    }

    #[test]
    fn uses_the_desktop_layout_when_the_width_is_unknown() {
        assert_eq!(layout(None, 272.0, false), DESKTOP_LAYOUT);
    }

    #[test]
    fn docks_both_panels_at_1440_and_above() {
        let mode = base(1440.0);
        assert_eq!(mode.sidebar, SidebarMode::Docked);
        assert_eq!(mode.details, DetailsMode::Docked);
    }

    #[test]
    fn docks_the_details_at_1280_only_while_the_main_area_keeps_640() {
        assert_eq!(base(1280.0).details, DetailsMode::Docked);
        assert_eq!(
            layout(Some(1280.0), 320.0, false).details,
            DetailsMode::Sheet
        );
    }

    #[test]
    fn docks_the_details_at_1280_when_both_panels_have_the_default_width() {
        let mode = decide_layout(LayoutInput {
            width: Some(1280.0),
            sidebar_width: PANEL_WIDTH,
            details_width: PANEL_WIDTH,
            coarse: false,
        });
        assert_eq!(mode.details, DetailsMode::Docked);
    }

    #[test]
    fn counts_a_collapsed_sidebar_as_no_width() {
        assert_eq!(
            layout(Some(1280.0), 0.0, false).details,
            DetailsMode::Docked
        );
    }

    #[test]
    fn makes_the_details_a_sheet_from_768_to_1279() {
        for width in [1279.0, 1024.0, 768.0] {
            assert_eq!(base(width).details, DetailsMode::Sheet, "{width}");
        }
    }

    #[test]
    fn makes_the_sidebar_a_drawer_under_1024() {
        assert_eq!(base(1024.0).sidebar, SidebarMode::Docked);
        assert_eq!(base(1023.0).sidebar, SidebarMode::Drawer);
        assert!(base(1023.0).compact);
    }

    #[test]
    fn makes_the_details_a_page_under_768() {
        assert_eq!(base(767.0).details, DetailsMode::Page);
        assert_eq!(base(390.0).details, DetailsMode::Page);
    }

    #[test]
    fn passes_the_pointer_type_through() {
        assert!(layout(Some(1440.0), 272.0, true).coarse);
    }

    #[test]
    fn limits_the_sidebar_to_30_percent_of_the_window_from_1024_to_1279() {
        assert_eq!(effective_sidebar_width(272.0, Some(1024.0)), 272.0);
        assert_eq!(effective_sidebar_width(440.0, Some(1100.0)), 330.0);
    }

    #[test]
    fn leaves_the_width_alone_elsewhere() {
        assert_eq!(effective_sidebar_width(440.0, Some(1280.0)), 440.0);
        assert_eq!(effective_sidebar_width(440.0, None), 440.0);
    }
}
