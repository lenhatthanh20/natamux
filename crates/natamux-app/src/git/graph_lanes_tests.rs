use super::*;

fn row(lane: usize, through: &[usize], into: &[usize], out: &[usize]) -> GraphRow {
    GraphRow {
        lane,
        through: through.to_vec(),
        into: into.to_vec(),
        out: out.to_vec(),
    }
}

fn commit(hash: &str) -> GraphCommit {
    GraphCommit {
        hash: hash.into(),
        ..GraphCommit::default()
    }
}

#[test]
fn puts_a_lane_in_the_middle_of_its_column() {
    assert_eq!(lane_x(0), 7.);
    assert_eq!(lane_x(2), 35.);
    assert_eq!(lanes_width(0), 14.);
    assert_eq!(lanes_width(3), 42.);
}

#[test]
fn draws_through_into_and_out_lines() {
    let found = links(&row(1, &[0], &[1, 2], &[1]));
    assert_eq!(found.len(), 4);
    assert_eq!(found[0].from, (7., 0.));
    assert_eq!(found[0].to, (7., 28.));
    assert_eq!(found[1].to, (21., 14.));
    assert_eq!(found[2].from, (35., 0.));
    assert_eq!(found[2].to, (21., 14.));
    assert_eq!(found[3].from, (21., 14.));
    assert_eq!(found[3].to, (21., 28.));
    let colors: Vec<usize> = found.iter().map(|link| link.color_lane).collect();
    assert_eq!(colors, [0, 1, 2, 1]);
}

#[test]
fn bends_a_line_between_lanes_only() {
    let found = links(&row(0, &[], &[1], &[0, 1]));
    assert_eq!(found[0].controls(), Some([(21., 7.), (7., 7.)]));
    assert_eq!(found[1].controls(), None);
    assert_eq!(found[2].controls(), Some([(7., 21.), (21., 21.)]));
}

#[test]
fn marks_the_newest_commit_and_the_head_hash() {
    let graph = GitGraph {
        head: "b".into(),
        ..GitGraph::default()
    };
    assert!(is_head(&graph, 0, &commit("a")));
    assert!(is_head(&graph, 1, &commit("b")));
    assert!(!is_head(&graph, 2, &commit("c")));
    assert_eq!(dot_radius(true), 5.);
    assert_eq!(dot_radius(false), 4.);
}
