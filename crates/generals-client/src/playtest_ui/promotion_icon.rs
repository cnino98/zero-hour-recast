use bevy::{
    asset::embedded_asset,
    prelude::*,
    render::render_resource::AsBindGroup,
    shader::ShaderRef,
    ui_render::{UiMaterialPlugin, ui_material::UiMaterial},
};

pub struct PromotionIconPlugin;

impl Plugin for PromotionIconPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "promotion_icon.wgsl");
        app.add_plugins(UiMaterialPlugin::<PromotionIcon>::default());
    }
}

// UiMaterial requires Clone; the other derives register the material and bindings.
#[derive(Asset, TypePath, AsBindGroup, Clone)]
pub struct PromotionIcon {
    #[uniform(0)]
    pub uv_rect: Vec4,
    #[uniform(1)]
    pub state: Vec4,
    #[uniform(2)]
    pub clock_color: Vec4,
    #[texture(3)]
    #[sampler(4)]
    pub image: Handle<Image>,
}

pub(super) fn update(
    icons: &mut Assets<PromotionIcon>,
    handle: &Handle<PromotionIcon>,
    next: PromotionIcon,
) {
    let Some(current) = icons.get(handle) else {
        return;
    };
    if current.image != next.image
        || current.uv_rect != next.uv_rect
        || current.state != next.state
        || current.clock_color != next.clock_color
    {
        *icons.get_mut(handle).expect("existing command icon") = next;
    }
}

impl UiMaterial for PromotionIcon {
    fn fragment_shader() -> ShaderRef {
        "embedded://generals_client/playtest_ui/promotion_icon.wgsl".into()
    }
}
