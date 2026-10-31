use super::*;

#[test]
fn the_open_or_focused_card_has_the_accent_border() {
    assert_eq!(card_border(true, false, true), CardBorder::Accent);
    assert_eq!(card_border(false, true, false), CardBorder::Accent);
}

#[test]
fn an_attention_card_has_its_state_border_and_others_none() {
    assert_eq!(card_border(false, false, true), CardBorder::Attention);
    assert_eq!(card_border(false, false, false), CardBorder::Plain);
}
