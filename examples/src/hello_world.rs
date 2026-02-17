use algoe::bivector::Bivector;
use nalgebra::Vector3;
use v4::builtin_components::mesh_component::VertexData;
use v4::ecs::compute::Compute;
use v4::ecs::material::{ShaderAttachment, ShaderTextureAttachment};
use v4::engine_support::texture_support::TextureProperties;
use v4::{
    V4,
    builtin_actions::EntityToggleAction,
    builtin_components::{
        camera_component::CameraComponent,
        mesh_component::{MeshComponent, VertexDescriptor},
        transform_component::TransformComponent,
    },
    component,
    ecs::{
        component::{ComponentDetails, ComponentId, ComponentSystem, UpdateParams},
        entity::EntityId,
    },
    engine_support::texture_support::TextureBundle,
    scene,
};
use wgpu::vertex_attr_array;

#[tokio::main]
pub async fn main() {
    let mut engine = V4::builder()
        .window_settings(600, 600, "Example V4 Project", None)
        .clear_color(wgpu::Color {
            r: 0.8,
            g: 0.15,
            b: 0.2,
            a: 1.0,
        })
        .features(wgpu::Features::POLYGON_MODE_LINE | wgpu::Features::IMMEDIATES)
        .limits(wgpu::Limits {
            max_immediate_size: 4,
            ..Default::default()
        })
        .hide_cursor(true)
        .build()
        .await;

    let rendering_manager = engine.rendering_manager();
    let device = rendering_manager.device();
    let queue = rendering_manager.queue();

    let (skybox_cubemap_tex, skybox_cubemap_output_bundle) = TextureBundle::create_texture(
        device,
        1024,
        1024,
        TextureProperties {
            format: wgpu::TextureFormat::Rgba32Float,
            storage_texture: Some(wgpu::StorageTextureAccess::WriteOnly),
            is_sampled: false,
            is_cubemap: true,
            is_filtered: false,
            extra_usages: wgpu::TextureUsages::TEXTURE_BINDING,
            ..Default::default()
        },
    );

    let skybox_display_bundle = TextureBundle::new(
        skybox_cubemap_tex.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::Cube),
            array_layer_count: Some(6),
            ..Default::default()
        }),
        TextureProperties {
            storage_texture: None,
            is_sampled: true,
            ..skybox_cubemap_output_bundle.properties()
        },
    );

    let mut skybox_compute = Compute::builder()
        .shader_path("./shaders/hello_world/skybox_compute.wgsl")
        .attachments(vec![
            ShaderAttachment::Texture(ShaderTextureAttachment {
                texture_bundle: TextureBundle::from_path(
                    "./assets/testing_textures/citrus_orchard_road_puresky_2k.hdr",
                    device,
                    queue,
                    TextureProperties {
                        format: wgpu::TextureFormat::Rgba32Float,
                        is_filtered: false,
                        is_sampled: false,
                        is_hdr: true,
                        ..Default::default()
                    },
                )
                .await
                .unwrap()
                .1,
                visibility: wgpu::ShaderStages::COMPUTE,
            }),
            ShaderAttachment::Texture(ShaderTextureAttachment {
                texture_bundle: skybox_cubemap_output_bundle,
                visibility: wgpu::ShaderStages::COMPUTE,
            }),
        ])
        .workgroup_counts(((1024 + 15) / 16, (1024 + 15) / 16, 6))
        .build();

    skybox_compute.initialize(device);

    rendering_manager.individual_compute_execution(&[skybox_compute]);

    scene! {
            scene: hello_scene,
            active_camera: "cam",
            /* screen_space_materials: [
                {
                    pipeline: {
                        fragment_shader_path: "shaders/hello_world/screen_space.wgsl",
                    }
                },
                {
                    pipeline: {
                        fragment_shader_path: "shaders/hello_world/screen_space_blur.wgsl"
                    }
                }
            ], */
            "cam_ent" = {
                components: [
                    CameraComponent(field_of_view: 80.0, aspect_ratio: 1.0, near_plane: 0.1, far_plane: 50.0, sensitivity: 0.002, movement_speed: 0.01, ident: "cam"),
                    TransformComponent(position: Vector3::new(0.0, 5.0, -5.0), rotation: Bivector::new(0.0, -std::f32::consts::FRAC_PI_4 / 2.0, 0.0).exponentiate(), uses_buffer: false),
                ]
            },
            "test_ent" = {
                material: {
                    pipeline: {
                        vertex_shader_path: "shaders/hello_world/point_vert.wgsl",
                        fragment_shader_path: "shaders/hello_world/point_frag.wgsl",
                        vertex_layouts: [Vertex::vertex_layout()],
                        uses_camera: false,
                        geometry_details: {
                            topology: wgpu::PrimitiveTopology::LineList,
                            polygon_mode: wgpu::PolygonMode::Line,
                        },
                    },
                },
                components: [
                    MeshComponent(
                        vertices: vec![vec![
                            Vertex::blank([0.0, 0.0, 0.0]),
                            Vertex::blank([0.0, 0.5, 0.0]),
                            Vertex::blank([0.3, -0.9, 0.0]),
                            Vertex::blank([-0.3, -0.3, 0.0])
                        ]],
                        enabled_models: vec![(0, None)]
                    ),
                    MeshComponent(
                        vertices: vec![vec![
                            Vertex::blank([-0.7, 0.0, 0.0]),
                            Vertex::blank([0.0, 0.2, 0.0]),
                            Vertex::blank([0.9, 0.9, 0.0]),
                            Vertex::blank([-0.3, -0.3, 0.0])
                        ]],
                        enabled_models: vec![(0, None)]
                    ),
                ],
                is_enabled: false,
            },
            "skybox" = {
                material: {
                    pipeline: {
                        vertex_shader_path: "shaders/hello_world/skybox_vertex.wgsl",
                        fragment_shader_path: "shaders/hello_world/skybox_fragment.wgsl",
                        vertex_layouts: [Vertex::vertex_layout()],
                        uses_camera: true,
                        render_priority: -1,
                    },
                    attachments: [
                        Texture(
                            texture_bundle: skybox_display_bundle,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                        )
                    ],
                    ident: "skybox_mat"
                },
                components: [

                    MeshComponent(
                        vertices: vec![
                            vec![
                                Vertex::blank([-1.0, 3.0, 1.0]),
                                Vertex::blank([-1.0, -1.0, 1.0]),
                                Vertex::blank([3.0, -1.0, 1.0])
                            ]
                        ],
                        indices: vec![
                            vec![0, 1, 2],
                        ],
                        enabled_models: vec![(0, None)]
                    ),
                ],
            },
            _ = {
                material: {
                    pipeline: {
                        vertex_shader_path: "shaders/hello_world/vertex.wgsl",
                        fragment_shader_path: "shaders/hello_world/fragment.wgsl",
                        vertex_layouts: [Vertex::vertex_layout(), TransformComponent::vertex_layout::<5>()],
                        uses_camera: true,
                        immediate_size: 4,
                    },
                    immediate_data: bytemuck::cast_slice(&[0.5_f32]).to_vec(),
                    attachments: [
                        Texture(
                            texture_bundle: TextureBundle::from_path(
                                "./assets/testing_textures/cube-diffuse.jpg",
                                device,
                                queue,
                                TextureProperties {
                                    format: wgpu::TextureFormat::Rgba8UnormSrgb,
                                    ..Default::default()
                                }
                            ).await.unwrap().1,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                        ),
                        Texture(
                            texture_bundle: TextureBundle::from_path(
                                "C:/Users/liors/CodingProjects/shaderbox/assets/shaderball_normal.jpg",
                                device,
                                queue,
                                TextureProperties {
                                    format: wgpu::TextureFormat::Rgba8Unorm,
                                    ..Default::default()
                                }
                            ).await.unwrap().1,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                        )
                    ],
                    ident: "immediate_mat"
                },
                components: [
                    TransformComponent(position: Vector3::new(0.0, 0.0, 0.0), ident: "thing"),
                    // MeshComponent<Vertex>::from_obj("assets/models/basic_cube.obj", true).ident("unused ident").await.unwrap(),
                    MeshComponent<Vertex>::from_obj("C:/Users/liors/CodingProjects/shaderbox/assets/shaderball.obj", true).ident("unused ident").await.unwrap(),
                    HideComponent(entity: ident("test_ent"), immediate_mat: ident("immediate_mat"))
                ],
            },
        }

    engine.attach_scene(hello_scene);

    engine.main_loop().await;
}

#[repr(C)]
#[derive(Debug, bytemuck::Pod, bytemuck::Zeroable, Clone, Copy, Default)]
struct Vertex {
    pos: [f32; 3],
    tex_coords: [f32; 2],
    normal: [f32; 3],
    tangent: [f32; 3],
    bitangent: [f32; 3],
}

impl Vertex {
    fn blank(pos: [f32; 3]) -> Self {
        Self {
            pos,
            ..Default::default()
        }
    }
}

impl VertexDescriptor for Vertex {
    const ATTRIBUTES: &[wgpu::VertexAttribute] = &vertex_attr_array![
        0 => Float32x3, 1 => Float32x2, 2 => Float32x3, 3 => Float32x3, 4 => Float32x3
    ];

    fn from_data(
        VertexData {
            pos,
            normal,
            tex_coords,
            tangent,
            bitangent,
        }: VertexData,
    ) -> Self {
        Self {
            pos,
            tex_coords,
            normal,
            tangent,
            bitangent,
        }
    }
}

#[component]
struct HideComponent {
    #[default(false)]
    showing: bool,
    entity: EntityId,
    immediate_mat: ComponentId,
}

impl ComponentSystem for HideComponent {
    fn update(
        &mut self,
        UpdateParams {
            input_manager,
            materials,
            ..
        }: UpdateParams<'_, '_>,
    ) -> v4::ecs::actions::ActionQueue {
        if input_manager.key_pressed(winit::keyboard::KeyCode::KeyT) {
            self.showing = !self.showing;
            /* if let Some(mat) = materials
                .iter_mut()
                .filter(|mat| mat.id() == self.mat)
                .next()
            {
                mat.set_enabled_state(self.showing);
            } */
            if let Some(mat) = materials
                .iter_mut()
                .filter(|mat| mat.id() == self.immediate_mat)
                .next()
            {
                mat.set_immediate_data(bytemuck::cast_slice(&[if self.showing {
                    1.0_f32
                } else {
                    0.5
                }]));
            }
            return vec![Box::new(EntityToggleAction(self.entity, None))];
        }
        Vec::new()
    }
}
