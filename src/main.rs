mod config;
mod discovery;
mod loader;
mod render;
mod ui;

use config::Config;
use discovery::{discover_wallpapers, read_stdin_path};
use loader::{AsyncImageLoader, load_image_thumbnail};
use render::{CardVertex, RenderState, Uniforms};
use ui::{FontManager, render_ui_overlay};

use std::io::{self, Write};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use ::wayland_client::WEnum;
use ::wayland_client::protocol::wl_pointer::ButtonState;
use layershellev::keyboard::{Key, KeyCode, NamedKey, PhysicalKey};
use layershellev::reexport::*;
use layershellev::xkb_keyboard::ElementState;
use layershellev::*;

struct SwallApp {
    config: Config,
    font_mgr: FontManager,

    wallpapers: Vec<PathBuf>,
    target_index: usize,
    current_scroll: f32,
    target_scroll: f32,

    render_state: Option<RenderState>,
    loader: Option<AsyncImageLoader>,

    cursor_pos: [f32; 2],
    screen_size: [f32; 2],
    overlay_dirty: bool,
    last_frame_time: Instant,
}

impl SwallApp {
    fn new(
        wallpapers: Vec<PathBuf>,
        start_index: usize,
        config: Config,
        font_mgr: FontManager,
    ) -> Self {
        Self {
            config,
            font_mgr,
            wallpapers,
            target_index: start_index,
            current_scroll: start_index as f32,
            target_scroll: start_index as f32,
            render_state: None,
            loader: None,
            cursor_pos: [0.0, 0.0],
            screen_size: [0.0, 0.0],
            overlay_dirty: true,
            last_frame_time: Instant::now(),
        }
    }

    fn select_prev(&mut self) {
        if self.target_index > 0 {
            self.target_index -= 1;
            self.target_scroll = self.target_index as f32;
            self.overlay_dirty = true;
            self.request_adjacent_textures();
        }
    }

    fn select_next(&mut self) {
        if self.target_index + 1 < self.wallpapers.len() {
            self.target_index += 1;
            self.target_scroll = self.target_index as f32;
            self.overlay_dirty = true;
            self.request_adjacent_textures();
        }
    }

    fn request_adjacent_textures(&mut self) {
        if let Some(loader) = &mut self.loader {
            let center = self.target_index;
            loader.request(center);
            for offset in 1..=4 {
                if center >= offset {
                    loader.request(center - offset);
                }
                if center + offset < self.wallpapers.len() {
                    loader.request(center + offset);
                }
            }
        }
    }

    fn confirm_selection(&self) {
        if let Some(path) = self.wallpapers.get(self.target_index) {
            let output = match path.canonicalize() {
                Ok(p) => p.display().to_string(),
                Err(_) => path.display().to_string(),
            };
            println!("{}", output);
            let _ = io::stdout().flush();
        }
        std::process::exit(0);
    }
}

fn main() {
    let config = Config::load();
    let font_mgr = FontManager::new(config.ui.font_spec());

    let stdin_path = read_stdin_path();
    let cli_args: Vec<PathBuf> = std::env::args().skip(1).map(PathBuf::from).collect();

    let (wallpapers, start_index) = match discover_wallpapers(&cli_args, stdin_path.as_deref()) {
        Ok(res) => res,
        Err(err) => {
            eprintln!("swall error: {}", err);
            std::process::exit(1);
        }
    };

    let ev: WindowState<()> = WindowState::new("swall")
        .with_use_display_handle(true)
        .with_layer(Layer::Overlay)
        .with_anchor(Anchor::Top | Anchor::Bottom | Anchor::Left | Anchor::Right)
        .with_keyboard_interacivity(KeyboardInteractivity::Exclusive)
        .with_exclusive_zone(-1)
        .with_size((0, 0))
        .build()
        .expect("Failed to create Wayland layer shell surface");

    let mut app = SwallApp::new(wallpapers, start_index, config, font_mgr);

    let _ = ev.running(move |event, state, unit_id| {
        if let LayerShellEvent::RequestMessages(msg) = event {
            match msg {
                DispatchMessage::RequestRefresh { width, height, .. } => {
                    let (sw, sh) = (*width as f32, *height as f32);
                    app.screen_size = [sw, sh];

                    if *width == 0 || *height == 0 {
                        return ReturnData::None;
                    }

                    // Initialize GPU render state on first refresh
                    if app.render_state.is_none() {
                        let wrapper = Arc::new(state.gen_mainwindow_wrapper());
                        let mut r_state =
                            pollster::block_on(RenderState::new(wrapper, *width, *height));

                        let max_thumb = app.config.cache.max_thumbnail_size;
                        let use_cache = app.config.cache.enabled;
                        if let Some(path) = app.wallpapers.get(app.target_index)
                            && let Ok((rgba, _, _)) =
                                load_image_thumbnail(path, max_thumb, use_cache)
                        {
                            let tex = RenderState::create_texture_from_rgba(
                                &r_state.device,
                                &r_state.queue,
                                &r_state.card_group_layout,
                                &rgba,
                                Some("initial-wallpaper"),
                            );
                            r_state.texture_cache.insert(app.target_index, tex);
                        }

                        let mut loader =
                            AsyncImageLoader::new(app.wallpapers.clone(), max_thumb, use_cache);
                        let center = app.target_index;
                        for offset in 1..=4 {
                            if center >= offset {
                                loader.request(center - offset);
                            }
                            if center + offset < app.wallpapers.len() {
                                loader.request(center + offset);
                            }
                        }

                        app.render_state = Some(r_state);
                        app.loader = Some(loader);
                        app.overlay_dirty = true;
                        app.last_frame_time = Instant::now();
                    }

                    let Some(r_state) = app.render_state.as_mut() else {
                        return ReturnData::None;
                    };

                    r_state.resize(*width, *height);

                    // Poll loader
                    if let Some(loader) = &app.loader {
                        while let Some(loaded) = loader.try_recv() {
                            if let Ok((rgba, _, _)) = loaded.result {
                                let tex = RenderState::create_texture_from_rgba(
                                    &r_state.device,
                                    &r_state.queue,
                                    &r_state.card_group_layout,
                                    &rgba,
                                    Some("card-texture"),
                                );
                                r_state.texture_cache.insert(loaded.index, tex);
                            }
                        }
                    }

                    // Animation update
                    let dt = app.last_frame_time.elapsed().as_secs_f32().min(0.1);
                    app.last_frame_time = Instant::now();

                    let diff = app.target_scroll - app.current_scroll;
                    let is_animating = diff.abs() > 0.0005;
                    if is_animating {
                        let factor = 1.0 - (-18.0 * dt).exp();
                        app.current_scroll += diff * factor;
                        if let Some(id) = unit_id {
                            state.request_refresh(id, RefreshRequest::NextFrame);
                        }
                    } else {
                        app.current_scroll = app.target_scroll;
                    }

                    // Uniforms update
                    let uniforms = Uniforms {
                        screen_size: [sw, sh],
                        _pad: [0.0, 0.0],
                        bg_color: app.config.colors.background_color.to_f32_array(),
                    };
                    r_state.queue.write_buffer(
                        &r_state.uniform_buffer,
                        0,
                        bytemuck::bytes_of(&uniforms),
                    );

                    // Update overlay UI
                    if app.overlay_dirty {
                        let current_file = app.wallpapers[app.target_index]
                            .file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or("wallpaper");

                        let overlay_img = render_ui_overlay(
                            *width,
                            *height,
                            app.target_index,
                            app.wallpapers.len(),
                            current_file,
                            &app.config,
                            &app.font_mgr,
                        );
                        r_state.update_overlay(&overlay_img);
                        app.overlay_dirty = false;
                    }

                    // Geometry computation for parallelogram cards (16:9 aspect ratio)
                    let cot75: f32 = 0.267_949_2;
                    let base_h = (sh * 0.45).clamp(240.0, 800.0);
                    let base_w = base_h * (16.0 / 9.0);
                    let gap = 48.0;
                    let step = base_w + gap;
                    let cy = sh * 0.5;

                    let mut vertices: Vec<CardVertex> = Vec::new();
                    let mut draw_commands: Vec<(usize, u32, u32)> = Vec::new();

                    for i in 0..app.wallpapers.len() {
                        let t = i as f32 - app.current_scroll;
                        let cx = sw * 0.5 + t * step;

                        let bound_w = base_w + base_h * cot75;
                        if cx + bound_w * 0.6 < 0.0 || cx - bound_w * 0.6 > sw {
                            continue;
                        }

                        if let Some(loader) = &mut app.loader
                            && !r_state.texture_cache.contains_key(&i)
                        {
                            loader.request(i);
                        }

                        let dist_center = t.abs();
                        let scale = 1.0 + 0.06 * (1.0 - dist_center.min(1.0));
                        let card_h = base_h * scale;
                        let card_w = base_w * scale;
                        let delta_x = card_h * cot75;

                        let base_unselected_opacity = app.config.colors.unselected_opacity;
                        let opacity = (1.0
                            - (1.0 - base_unselected_opacity) * (dist_center / 2.0).min(1.0))
                        .max(0.15);

                        let (border_color, border_width) = if i == app.target_index {
                            (
                                app.config.colors.selected_border_color.to_f32_array(),
                                app.config.colors.selected_border_width * scale,
                            )
                        } else {
                            (
                                app.config.colors.card_border_color.to_f32_array(),
                                app.config.colors.card_border_width,
                            )
                        };

                        let tl_local = [delta_x * 0.5 - card_w * 0.5, -card_h * 0.5];
                        let tr_local = [delta_x * 0.5 + card_w * 0.5, -card_h * 0.5];
                        let br_local = [-delta_x * 0.5 + card_w * 0.5, card_h * 0.5];
                        let bl_local = [-delta_x * 0.5 - card_w * 0.5, card_h * 0.5];

                        let tl_pos = [cx + tl_local[0], cy + tl_local[1]];
                        let tr_pos = [cx + tr_local[0], cy + tr_local[1]];
                        let br_pos = [cx + br_local[0], cy + br_local[1]];
                        let bl_pos = [cx + bl_local[0], cy + bl_local[1]];

                        let (tw, th) = if let Some(tex) = r_state.texture_cache.get(&i) {
                            (tex.width as f32, tex.height as f32)
                        } else {
                            (
                                r_state.fallback_texture.width as f32,
                                r_state.fallback_texture.height as f32,
                            )
                        };

                        let scale_cov = (card_h / th).max((card_w + delta_x) / tw);
                        let eff_w = tw * scale_cov;
                        let eff_h = th * scale_cov;

                        let calc_uv = |local: [f32; 2]| -> [f32; 2] {
                            [
                                (0.5 + local[0] / eff_w).clamp(0.0, 1.0),
                                (0.5 + local[1] / eff_h).clamp(0.0, 1.0),
                            ]
                        };

                        let v_tl = CardVertex {
                            pos: tl_pos,
                            uv: calc_uv(tl_local),
                            local_pos: tl_local,
                            card_size: [card_w, card_h],
                            border_color,
                            border_width,
                            opacity,
                        };
                        let v_tr = CardVertex {
                            pos: tr_pos,
                            uv: calc_uv(tr_local),
                            local_pos: tr_local,
                            card_size: [card_w, card_h],
                            border_color,
                            border_width,
                            opacity,
                        };
                        let v_br = CardVertex {
                            pos: br_pos,
                            uv: calc_uv(br_local),
                            local_pos: br_local,
                            card_size: [card_w, card_h],
                            border_color,
                            border_width,
                            opacity,
                        };
                        let v_bl = CardVertex {
                            pos: bl_pos,
                            uv: calc_uv(bl_local),
                            local_pos: bl_local,
                            card_size: [card_w, card_h],
                            border_color,
                            border_width,
                            opacity,
                        };

                        let v_start = vertices.len() as u32;
                        vertices.extend_from_slice(&[v_tl, v_tr, v_br, v_tl, v_br, v_bl]);
                        draw_commands.push((i, v_start, 6));
                    }

                    if !vertices.is_empty() {
                        r_state.queue.write_buffer(
                            &r_state.card_vertex_buffer,
                            0,
                            bytemuck::cast_slice(&vertices),
                        );
                    }

                    // Render pass execution
                    let surface_texture = match r_state.surface.get_current_texture() {
                        Ok(t) => t,
                        Err(wgpu::SurfaceError::Outdated) => {
                            r_state.surface.configure(&r_state.device, &r_state.config);
                            return ReturnData::None;
                        }
                        Err(e) => {
                            eprintln!("Surface error: {:?}", e);
                            return ReturnData::None;
                        }
                    };

                    let view = surface_texture
                        .texture
                        .create_view(&wgpu::TextureViewDescriptor::default());

                    let mut encoder =
                        r_state
                            .device
                            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                                label: Some("frame-encoder"),
                            });

                    {
                        let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                            label: Some("swall-render-pass"),
                            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                                view: &view,
                                resolve_target: None,
                                ops: wgpu::Operations {
                                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                                    store: wgpu::StoreOp::Store,
                                },
                            })],
                            depth_stencil_attachment: None,
                            timestamp_writes: None,
                            occlusion_query_set: None,
                        });

                        // 1. Draw Background
                        rpass.set_pipeline(&r_state.bg_pipeline);
                        rpass.set_bind_group(0, &r_state.bg_bind_group, &[]);
                        rpass.draw(0..3, 0..1);

                        // 2. Draw Cards
                        rpass.set_pipeline(&r_state.card_pipeline);
                        rpass.set_bind_group(0, &r_state.card_global_bind_group, &[]);
                        rpass.set_vertex_buffer(0, r_state.card_vertex_buffer.slice(..));

                        for (card_idx, v_start, v_count) in &draw_commands {
                            let bind_group = if let Some(tex) = r_state.texture_cache.get(card_idx)
                            {
                                &tex.bind_group
                            } else {
                                &r_state.fallback_texture.bind_group
                            };
                            rpass.set_bind_group(1, bind_group, &[]);
                            rpass.draw(*v_start..(*v_start + *v_count), 0..1);
                        }

                        // 3. Draw Overlay
                        if (app.config.ui.show_top_bar || app.config.ui.show_bottom_bar)
                            && let Some(overlay_tex) = &r_state.overlay_texture
                        {
                            rpass.set_pipeline(&r_state.overlay_pipeline);
                            rpass.set_bind_group(0, &overlay_tex.bind_group, &[]);
                            rpass.draw(0..3, 0..1);
                        }
                    }

                    r_state.queue.submit(Some(encoder.finish()));
                    surface_texture.present();
                }

                DispatchMessage::KeyboardInput { event, .. } => {
                    if event.state == ElementState::Pressed {
                        let is_left = match event.logical_key {
                            Key::Named(NamedKey::ArrowLeft) => true,
                            Key::Character(ref s) if s.eq_ignore_ascii_case("h") => true,
                            _ => {
                                event.physical_key == PhysicalKey::Code(KeyCode::ArrowLeft)
                                    || event.physical_key == PhysicalKey::Code(KeyCode::KeyH)
                            }
                        };

                        let is_right = match event.logical_key {
                            Key::Named(NamedKey::ArrowRight) => true,
                            Key::Character(ref s) if s.eq_ignore_ascii_case("l") => true,
                            _ => {
                                event.physical_key == PhysicalKey::Code(KeyCode::ArrowRight)
                                    || event.physical_key == PhysicalKey::Code(KeyCode::KeyL)
                            }
                        };

                        let is_enter = match event.logical_key {
                            Key::Named(NamedKey::Enter) => true,
                            _ => {
                                event.physical_key == PhysicalKey::Code(KeyCode::Enter)
                                    || event.physical_key == PhysicalKey::Code(KeyCode::NumpadEnter)
                            }
                        };

                        let is_esc = match event.logical_key {
                            Key::Named(NamedKey::Escape) => true,
                            _ => event.physical_key == PhysicalKey::Code(KeyCode::Escape),
                        };

                        if is_left {
                            app.select_prev();
                            if let Some(id) = unit_id {
                                state.request_refresh(id, RefreshRequest::NextFrame);
                            }
                        } else if is_right {
                            app.select_next();
                            if let Some(id) = unit_id {
                                state.request_refresh(id, RefreshRequest::NextFrame);
                            }
                        } else if is_enter {
                            app.confirm_selection();
                        } else if is_esc {
                            std::process::exit(0);
                        }
                    }
                }

                DispatchMessage::MouseMotion {
                    surface_x,
                    surface_y,
                    ..
                } => {
                    app.cursor_pos = [*surface_x as f32, *surface_y as f32];
                }

                DispatchMessage::MouseButton {
                    state: btn_state,
                    button,
                    ..
                } => {
                    if *btn_state == WEnum::Value(ButtonState::Pressed) && *button == 272 {
                        let (sw, sh) = (app.screen_size[0], app.screen_size[1]);
                        let card_h = (sh * 0.45).clamp(240.0, 800.0);
                        let card_w = card_h * (16.0 / 9.0);
                        let cot75: f32 = 0.267_949_2;
                        let gap = 48.0;
                        let step = card_w + gap;
                        let cy = sh * 0.5;

                        let [mx, my] = app.cursor_pos;
                        let mut clicked_idx = None;

                        for i in 0..app.wallpapers.len() {
                            let t = i as f32 - app.current_scroll;
                            let cx = sw * 0.5 + t * step;
                            let px = mx - cx;
                            let py = my - cy;

                            let dist_horizontal = card_h * 0.5 - py.abs();
                            let u_slant = px + py * cot75;
                            let dist_slant = card_w * 0.5 - u_slant.abs();

                            if dist_horizontal >= 0.0 && dist_slant >= 0.0 {
                                clicked_idx = Some(i);
                                break;
                            }
                        }

                        if let Some(idx) = clicked_idx {
                            if idx == app.target_index {
                                app.confirm_selection();
                            } else {
                                app.target_index = idx;
                                app.target_scroll = idx as f32;
                                app.overlay_dirty = true;
                                app.request_adjacent_textures();
                                if let Some(id) = unit_id {
                                    state.request_refresh(id, RefreshRequest::NextFrame);
                                }
                            }
                        }
                    }
                }

                DispatchMessage::Axis { vertical, .. } => {
                    let scroll_y = if vertical.discrete != 0 {
                        vertical.discrete as f64
                    } else {
                        vertical.absolute
                    };
                    if scroll_y > 0.1 {
                        app.select_next();
                        if let Some(id) = unit_id {
                            state.request_refresh(id, RefreshRequest::NextFrame);
                        }
                    } else if scroll_y < -0.1 {
                        app.select_prev();
                        if let Some(id) = unit_id {
                            state.request_refresh(id, RefreshRequest::NextFrame);
                        }
                    }
                }

                DispatchMessage::Closed => {
                    std::process::exit(0);
                }

                _ => {}
            }
        }

        ReturnData::None
    });
}
