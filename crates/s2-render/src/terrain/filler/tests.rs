use super::*;
use crate::terrain::testing::{shading, texture};

fn vertex(x: i32, y: i32, u: i32, v: i32, light: i32) -> Vertex {
    Vertex {
        x: Subpixel::from_raw(x),
        y: Subpixel::from_raw(y),
        u: Fixed8::from_raw(u),
        v: Fixed8::from_raw(v),
        light: Fixed8::from_raw(light),
    }
}

/// A corner at whole pixel (`x`, `y`) with whole texel (`u`, `v`) and shading row `light`.
fn corner(x: i32, y: i32, u: i32, v: i32, light: i32) -> Vertex {
    vertex(x << 4, y << 4, u << 8, v << 8, light << 8)
}

/// Shading table where every row maps a texel to itself.
fn identity_shading() -> ShadingTable {
    shading(|_, t| t)
}

/// Shading table where every texel shades to its row number.
fn row_shading() -> ShadingTable {
    shading(|level, _| level)
}

/// The colour a [`Canvas`] starts out with, which no test draws.
const UNTOUCHED: u8 = 238;

struct Canvas {
    pixels: Vec<u8>,
    width: u32,
    height: u32,
}

impl Canvas {
    fn new(width: u32, height: u32) -> Self {
        Self {
            pixels: vec![UNTOUCHED; (width * height) as usize],
            width,
            height,
        }
    }

    fn surface(&mut self) -> Surface<'_> {
        Surface::new(&mut self.pixels, self.width, self.height).unwrap()
    }

    fn at(&self, x: u32, y: u32) -> u8 {
        self.pixels[(y * self.width + x) as usize]
    }

    fn drawn(&self) -> usize {
        self.pixels.iter().filter(|&&p| p != UNTOUCHED).count()
    }
}

#[test]
fn clip_point_lands_on_the_bound_and_interpolates_from_the_inside_end() {
    let inside = vertex(16, 32, 16 << 8, 32 << 8, 64 << 8);
    let outside = vertex(-48, 96, 32 << 8, 16 << 8, 0);
    let p = clip_point(inside, outside, Axis::X, Subpixel::from_raw(0));
    assert_eq!(p.x.raw(), 0);
    assert_eq!(p.y.raw(), 48);
    assert_eq!(p.u.raw(), 20 << 8);
    assert_eq!(p.v.raw(), 28 << 8);
    assert_eq!(p.light.raw(), 48 << 8);
}

#[test]
fn clip_point_truncates_towards_zero() {
    let inside = vertex(0, 0, 0, 0, 0);
    let outside = vertex(48, 0, 0, 0, -10);
    let p = clip_point(inside, outside, Axis::X, Subpixel::from_raw(16));
    assert_eq!(p.light.raw(), -3, "-10 / 3 truncates to -3");
    let outside = vertex(48, 0, 0, 0, 10);
    assert_eq!(
        clip_point(inside, outside, Axis::X, Subpixel::from_raw(16))
            .light
            .raw(),
        3
    );
}

#[test]
fn clip_edge_cases() {
    let bounds = Bounds::for_extent(10);
    let inside = |x| vertex(x, 0, x, 0, 0);
    assert_eq!(
        clip_edge(inside(16), inside(32), Axis::X, bounds),
        [Some(inside(16)), None]
    );
    let moved = clip_edge(inside(-16), inside(16), Axis::X, bounds);
    assert_eq!(
        moved,
        [Some(inside(0)), None],
        "the outside start moves onto the bound"
    );
    let leaving = clip_edge(inside(150), inside(170), Axis::X, bounds);
    assert_eq!(leaving, [Some(inside(150)), Some(inside(160))]);
    assert_eq!(clip_edge(inside(-5), inside(-50), Axis::X, bounds), [None, None]);
    assert_eq!(
        clip_edge(inside(0), inside(-1), Axis::X, bounds),
        [None, None],
        "a coordinate on the low bound is outside"
    );
    assert_eq!(
        clip_edge(inside(160), inside(200), Axis::X, bounds),
        [None, None],
        "a coordinate on the high bound is outside"
    );
    let across = clip_edge(inside(200), inside(-40), Axis::X, bounds);
    assert_eq!(across, [Some(inside(160)), Some(inside(0))]);
}

#[test]
fn attribute_steps_rotate_the_sign_into_the_lowest_bit() {
    let step = |a, b, rows| attribute_step(Fixed8::from_raw(a), Fixed8::from_raw(b), rows).raw();
    assert_eq!(step(0, 0, 3), 0);
    assert_eq!(step(0, 1 << 8, 1), 1 << 24);
    assert_eq!(step(0, 3 << 8, 2), 3 << 23);
    assert_eq!(step(1, 0, 1), -(1 << 16) + 1, "-1.0 comes out as -1 + 1/65536");
    assert_eq!(step(1 << 8, 0, 1), -(1 << 24) + 1);
}

#[test]
fn edge_setup_counts_whole_rows_and_steps_x_exactly() {
    let top = vertex(8, 8, 0, 0, 0);
    let bottom = vertex(8 + 48, 8 + 64, 4 << 8, 0, 0);
    let walk = edge_setup(top, bottom).unwrap();
    assert_eq!((walk.first_row, walk.rows), (0, 4));
    assert_eq!(walk.start.words(), [0, 0, 0, 1 << 31]);
    let x = Fixed16::from_raw(3 << 14);
    let u = attribute_step(top.u, bottom.u, 4);
    assert_eq!(
        walk.step,
        Packed::steps(x, u, Fixed16::default(), Fixed16::default())
    );
    assert!(edge_setup(vertex(0, 16, 0, 0, 0), vertex(0, 31, 0, 0, 0)).is_none());
    assert!(edge_setup(vertex(0, 31, 0, 0, 0), vertex(0, 16, 0, 0, 0)).is_none());
}

#[test]
fn span_setup_starts_at_the_right_edge() {
    let right = EdgeEntry {
        x: 10,
        u: (18 << 8) + 52,
        v: (86 << 8) + 120,
        light: (64 << 8) + 192,
    };
    let left = EdgeEntry {
        x: 6,
        u: (22 << 8) + 52,
        v: (82 << 8) + 120,
        light: (60 << 8) + 192,
    };
    let span = span_setup(right, left, false).unwrap();
    assert_eq!((span.right, span.pixels), (10, 4));
    assert_eq!(
        span.cursor,
        SpanCursor {
            u: 18,
            u_acc: (52 << 24) + 18,
            v: 86,
            v_frac: 120 << 8,
            light: 64,
            light_frac: 192 << 8,
        }
    );
    assert_eq!(
        span.step,
        SpanStep {
            u: 1,
            u_acc: (u32::from(u8::MAX) << 8) + 1,
            v: u8::MAX,
            v_frac: 0,
            light: u8::MAX,
            light_frac: 0,
        }
    );
    assert!(span_setup(left, right, false).is_none());
    assert!(span_setup(right, right, false).is_none());
}

#[test]
fn keyed_spans_count_pixels_in_a_byte() {
    let entry = |x| EdgeEntry {
        x,
        ..EdgeEntry::default()
    };
    assert_eq!(span_setup(entry(300), entry(0), false).unwrap().pixels, 300);
    assert_eq!(span_setup(entry(300), entry(0), true).unwrap().pixels, 44);
    assert_eq!(span_setup(entry(256), entry(0), true).unwrap().pixels, 1);
}

#[test]
fn pixel_steps_carry_each_fraction_into_its_integer() {
    let cursor = SpanCursor {
        u: u8::MAX,
        u_acc: u32::from(u16::MAX) << 16,
        v: 7,
        v_frac: u16::MAX,
        light: 64,
        light_frac: 1 << 15,
    };
    let step = SpanStep {
        u: 0,
        u_acc: 1 << 16,
        v: 0,
        v_frac: 1,
        light: u8::MAX,
        light_frac: 1 << 15,
    };
    let next = cursor.step(step);
    assert_eq!((next.u, next.u_acc), (0, 0), "u wraps round the sheet");
    assert_eq!((next.v, next.v_frac), (8, 0));
    assert_eq!((next.light, next.light_frac), (64, 0), "-1 plus the carry");
}

#[test]
fn the_low_half_of_the_u_register_leaks_into_the_u_fraction() {
    let cursor = SpanCursor {
        u: 3,
        u_acc: u32::from(u8::MAX) << 8,
        v: 0,
        v_frac: 0,
        light: 0,
        light_frac: 0,
    };
    let step = SpanStep {
        u: 0,
        u_acc: 1 << 8,
        v: 1,
        v_frac: 0,
        light: 0,
        light_frac: 0,
    };
    assert_eq!(cursor.step(step).u_acc, 1 << 16);
    assert_eq!(cursor.texel(), 3);
}

#[test]
fn fills_the_rows_between_its_edges_right_to_left() {
    let texture = texture(|_, _| 5);
    let shading = identity_shading();
    let mut canvas = Canvas::new(16, 16);
    let corners = [
        corner(0, 0, 0, 0, 0),
        corner(8, 8, 0, 0, 0),
        corner(0, 8, 0, 0, 0),
    ];
    let material = Material {
        texture: &texture,
        shading: &shading,
    };
    fill(&mut canvas.surface(), material, &corners, None);
    for y in 0..16 {
        for x in 0..16 {
            let inside = y < 8 && x < y;
            assert_eq!(canvas.at(x, y) == 5, inside, "({x}, {y})");
        }
    }
    assert_eq!(canvas.drawn(), 28);
}

#[test]
fn counter_clockwise_polygons_draw_nothing() {
    let texture = texture(|_, _| 5);
    let shading = identity_shading();
    let mut canvas = Canvas::new(16, 16);
    let corners = [
        corner(0, 0, 0, 0, 0),
        corner(0, 8, 0, 0, 0),
        corner(8, 8, 0, 0, 0),
    ];
    let material = Material {
        texture: &texture,
        shading: &shading,
    };
    fill(&mut canvas.surface(), material, &corners, None);
    assert_eq!(canvas.drawn(), 0);
}

#[test]
fn samples_the_right_edge_exactly_and_never_the_left() {
    let texture = texture(|u, _| u);
    let shading = identity_shading();
    let mut canvas = Canvas::new(32, 4);
    let corners = [
        corner(0, 0, 0, 0, 0),
        corner(16, 0, 32, 0, 0),
        corner(16, 4, 32, 0, 0),
        corner(0, 4, 0, 0, 0),
    ];
    let material = Material {
        texture: &texture,
        shading: &shading,
    };
    fill(&mut canvas.surface(), material, &corners, None);
    let row: Vec<u8> = (0..16).map(|x| canvas.at(x, 1)).collect();
    let expected: Vec<u8> = (0..16).map(|x| 32 - 2 * (15 - x)).collect();
    assert_eq!(row, expected);
}

#[test]
fn shading_rows_follow_the_light() {
    let texture = texture(|_, _| 1);
    let shading = row_shading();
    let mut canvas = Canvas::new(8, 2);
    let corners = [
        corner(0, 0, 0, 0, 10),
        corner(8, 0, 0, 0, 18),
        corner(8, 2, 0, 0, 18),
        corner(0, 2, 0, 0, 10),
    ];
    let material = Material {
        texture: &texture,
        shading: &shading,
    };
    fill(&mut canvas.surface(), material, &corners, None);
    let row: Vec<u8> = (0..8).map(|x| canvas.at(x, 0)).collect();
    assert_eq!(row, [11, 12, 13, 14, 15, 16, 17, 18]);
}

#[test]
fn the_key_leaves_pixels_alone() {
    let texture = texture(|u, _| if u % 2 == 0 { 0 } else { 9 });
    let shading = identity_shading();
    let mut canvas = Canvas::new(8, 2);
    let corners = [
        corner(0, 0, 0, 0, 0),
        corner(8, 0, 8, 0, 0),
        corner(8, 2, 8, 0, 0),
        corner(0, 2, 0, 0, 0),
    ];
    let material = Material {
        texture: &texture,
        shading: &shading,
    };
    fill(&mut canvas.surface(), material, &corners, Some(0));
    let row: Vec<u8> = (0..8).map(|x| canvas.at(x, 0)).collect();
    assert_eq!(row, [9, UNTOUCHED, 9, UNTOUCHED, 9, UNTOUCHED, 9, UNTOUCHED]);
}

#[test]
fn clips_to_the_surface() {
    let texture = texture(|_, _| 3);
    let shading = identity_shading();
    let material = Material {
        texture: &texture,
        shading: &shading,
    };
    let mut canvas = Canvas::new(20, 10);
    let huge = [
        corner(-1000, -1000, 0, 0, 0),
        corner(1000, -1000, 0, 0, 0),
        corner(1000, 1000, 0, 0, 0),
        corner(-1000, 1000, 0, 0, 0),
    ];
    fill(&mut canvas.surface(), material, &huge, None);
    assert_eq!(canvas.drawn(), 200, "covers every pixel");

    let mut canvas = Canvas::new(20, 10);
    let away = [
        corner(30, 0, 0, 0, 0),
        corner(40, 5, 0, 0, 0),
        corner(30, 5, 0, 0, 0),
    ];
    fill(&mut canvas.surface(), material, &away, None);
    assert_eq!(canvas.drawn(), 0);

    let mut canvas = Canvas::new(20, 10);
    let partly = [
        corner(10, -5, 0, 0, 0),
        corner(30, 15, 0, 0, 0),
        corner(-10, 15, 0, 0, 0),
    ];
    fill(&mut canvas.surface(), material, &partly, None);
    assert!(canvas.drawn() > 0);
}

#[test]
fn rejects_too_many_corners() {
    let texture = texture(|_, _| 3);
    let shading = identity_shading();
    let material = Material {
        texture: &texture,
        shading: &shading,
    };
    let mut canvas = Canvas::new(8, 8);
    let five = [corner(0, 0, 0, 0, 0); 5];
    fill(&mut canvas.surface(), material, &five, None);
    assert_eq!(canvas.drawn(), 0);
}

/// The straightforward form of the scan converter: every edge writes its rows into a table
/// indexed by screen row, later edges overwriting earlier ones, and the spans read the table.
fn reference_fill(surface: &mut Surface<'_>, material: Material<'_>, corners: &[Vertex], key: Option<u8>) {
    let polygon = Polygon::new(corners)
        .unwrap()
        .clip(Axis::X, Bounds::for_extent(surface.width()))
        .clip(Axis::Y, Bounds::for_extent(surface.height()));
    if polygon.len < 2 {
        return;
    }
    let mut table = vec![[EdgeEntry::default(); 2]; surface.height() as usize + 1];
    let mut top = i32::MAX;
    let mut bottom = 0;
    for (a, b) in polygon.edges() {
        let (half, from, to) = match b.y.cmp(&a.y) {
            std::cmp::Ordering::Greater => (0, a, b),
            std::cmp::Ordering::Less => (1, b, a),
            std::cmp::Ordering::Equal => continue,
        };
        let Some(walk) = edge_setup(from, to) else {
            continue;
        };
        top = top.min(from.y.raw());
        bottom = bottom.max(to.y.raw());
        let mut at = walk.start;
        for row in walk.first_row..walk.first_row + walk.rows {
            table[usize::try_from(row).unwrap()][half] = at.entry();
            at = at.step(walk.step);
        }
    }
    for row in (top >> 4)..(bottom >> 4) {
        let [right, left] = table[usize::try_from(row).unwrap()];
        if let Some(span) = span_setup(right, left, key.is_some()) {
            draw_span(surface, material, row, span, key);
        }
    }
}

#[test]
fn matches_the_scan_line_table_on_random_triangles() {
    let texture = texture(|u, v| u ^ v.rotate_left(3));
    let shading = shading(|level, t| t.wrapping_add(level));
    let material = Material {
        texture: &texture,
        shading: &shading,
    };
    let mut seed = 0x2545_f491_4f6c_dd1d_u64;
    let mut next = |range: i32| {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        i32::try_from(seed >> 33).unwrap() % range
    };
    for round in 0..20_000 {
        let mut corners = [Vertex::default(); 3];
        for c in &mut corners {
            *c = vertex(
                next(1600) - 400,
                next(1200) - 300,
                next(1 << 16),
                next(1 << 16),
                next(1 << 15),
            );
        }
        let key = (round % 3 == 0).then_some(0);
        let mut ours = Canvas::new(50, 40);
        let mut theirs = Canvas::new(50, 40);
        fill(&mut ours.surface(), material, &corners, key);
        reference_fill(&mut theirs.surface(), material, &corners, key);
        assert!(ours.pixels == theirs.pixels, "round {round}: {corners:?}");
    }
}
