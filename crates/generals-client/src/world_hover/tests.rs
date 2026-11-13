use super::*;
use crate::{
    skirmish_verification::{
        advance_until, construct, construct_type, friendly_entity, produce_type,
    },
    strategy_verification::template,
};
use generals_sim::skirmish::{Faction, Kind};

#[test]
fn tooltip_panel_loads_after_startup_and_retires_with_its_map_font() {
    let mut app = App::new();
    app.init_resource::<Assets<Font>>()
        .add_systems(Update, spawn);
    app.update();
    assert!(
        app.world_mut()
            .query::<&Panel>()
            .iter(app.world())
            .next()
            .is_none()
    );
    app.insert_resource(test_style());
    app.update();
    let (old_panel, old_font) = {
        let mut panels = app.world_mut().query::<(Entity, &Panel)>();
        let (entity, panel) = panels.single(app.world()).unwrap();
        (entity, panel.0)
    };
    let old_children: Vec<_> = app
        .world()
        .get::<Children>(old_panel)
        .unwrap()
        .iter()
        .collect();
    app.update();
    assert_eq!(app.world().get::<Panel>(old_panel).unwrap().0, old_font);
    app.insert_resource(test_style());
    app.update();
    assert!(app.world().get::<Panel>(old_panel).is_none());
    for child in old_children {
        assert!(app.world().get_entity(child).is_err());
    }
    assert!(
        app.world()
            .resource::<Assets<Font>>()
            .get(old_font)
            .is_none()
    );
    assert_eq!(app.world().resource::<Assets<Font>>().len(), 1);
    assert!(
        app.world_mut()
            .query::<&Panel>()
            .single(app.world())
            .is_ok()
    );
}

fn test_style() -> Style {
    Style {
        font: Some(Font::from_bytes(Vec::new())),
        points: 8.0,
        font_adjustment: 0.7,
        delay: Duration::from_millis(800),
        fill: Duration::from_millis(250),
        width_fraction: 0.2,
        animate_background: false,
        text: [220, 220, 220, 255],
        highlight: Color::WHITE,
        shadow: Color::BLACK,
        background: [20, 20, 20, 255],
        border: Color::BLACK,
        player_colors: [false, true],
        adjust_color: true,
    }
}

#[test]
fn cursor_hover_delay_restarts_after_movement_or_a_new_object() {
    let mut hover = Hover::default();
    let point = Some(Vec2::new(110.0, 75.0));
    let target = Some((3, 2));
    let delay = Duration::from_millis(800);
    let time = Duration::from_millis;
    assert_eq!(hover.elapsed(time(10), target, point, delay), None);
    assert_eq!(hover.elapsed(time(809), target, point, delay), None);
    assert_eq!(
        hover.elapsed(time(810), target, point, delay),
        Some(time(0))
    );
    assert_eq!(
        hover.elapsed(time(920), target, point, delay),
        Some(time(110))
    );
    let moved = Some(Vec2::new(111.0, 75.0));
    assert_eq!(hover.elapsed(time(921), target, moved, delay), None);
    assert_eq!(
        hover.elapsed(time(1721), target, moved, delay),
        Some(time(0))
    );
    assert_eq!(hover.elapsed(time(1722), Some((3, 8)), moved, delay), None);
    assert_eq!(hover.elapsed(time(2521), Some((3, 8)), moved, delay), None);
    assert_eq!(
        hover.elapsed(time(2522), Some((3, 8)), moved, delay),
        Some(time(0))
    );
    assert_eq!(hover.elapsed(time(2523), Some((4, 8)), moved, delay), None);
}

#[test]
fn cursor_tooltip_switches_sides_at_the_display_edges() {
    let screen = Vec2::new(800.0, 600.0);
    let size = Vec2::new(55.0, 10.0);
    let placement = |x, y| {
        tooltip_origin(Vec2::new(x, y), size, screen)
            .to_array()
            .map(f32::to_bits)
    };
    assert_eq!(
        placement(400.0, 300.0),
        [420.0_f32.to_bits(), 300.0_f32.to_bits()]
    );
    assert_eq!(
        placement(790.0, 590.0),
        [735.0_f32.to_bits(), 580.0_f32.to_bits()]
    );
}

#[test]
fn cursor_tooltip_draws_the_previous_highlight_position() {
    let mut hover = Hover::default();
    let style = test_style();
    let width = 100.0;
    let reveal = |hover: &mut Hover, millis| {
        hover
            .reveal(&style, width, Duration::from_millis(millis))
            .to_bits()
    };
    assert_eq!(reveal(&mut hover, 0), 0.0_f32.to_bits());
    assert_eq!(reveal(&mut hover, 125), 0.0_f32.to_bits());
    assert_eq!(reveal(&mut hover, 250), 50.0_f32.to_bits());
    assert_eq!(reveal(&mut hover, 300), 100.0_f32.to_bits());
    assert_eq!(reveal(&mut hover, 301), 115.0_f32.to_bits());
}

#[ignore = "requires original game installation"]
#[test]
fn installed_paid_bomb_truck_hover_uses_the_immediate_disguise_for_an_active_enemy() {
    let (assets, mut battle) = crate::cleanup_verification::tests::installed();
    let style = load(&assets).unwrap();
    assert_eq!(style.delay, Duration::from_millis(800));
    assert_eq!(style.fill, Duration::from_millis(250));
    assert!(style.player_colors[1]);
    let game = &mut battle.simulation;
    game.reset(Faction::Usa, Faction::Gla).unwrap();
    let builder = friendly_entity(game, Kind::Builder, 1).unwrap();
    construct(game, builder, Kind::SupplyCenter, 1).unwrap();
    construct(game, builder, Kind::Barracks, 1).unwrap();
    let dealer = construct(game, builder, Kind::Factory, 1).unwrap();
    let palace = template(game, "GLAPalace").unwrap();
    construct_type(game, builder, palace, 1).unwrap();
    let truck_type = template(game, "GLAVehicleBombTruck").unwrap();
    let technical_type = template(game, "GLAVehicleTechnical").unwrap();
    let cost = game.catalog[truck_type].cost + game.catalog[technical_type].cost;
    advance_until(game, 18_000, "Bomb Truck and Technical income", |game| {
        game.players[1].cash >= cost
    })
    .unwrap();
    let truck = produce_type(game, dealer, truck_type, 1).unwrap();
    let technical = produce_type(game, dealer, technical_type, 1).unwrap();
    game.bomb_truck_order_disguise(truck, technical).unwrap();
    advance_until(
        game,
        120,
        "Bomb Truck hover target before drawable swap",
        |game| game.entities[truck].disguise_hover_appearance().is_some(),
    )
    .unwrap();
    assert!(game.entities[truck].disguised_appearance().is_none());
    let apparent = identity(game, truck, 0);
    assert_eq!(apparent.type_index, game.entities[technical].type_index);
    assert_eq!(apparent.owner, game.entities[technical].owner);
    assert_eq!(identity(game, truck, 1).type_index, truck_type);
    assert_eq!(
        identity(game, truck, game.neutral_owner()).type_index,
        truck_type
    );
}
