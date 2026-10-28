# Research: ruffle-embed (verified)

## Recommendation

الخلاصة: يمكن تضمين Ruffle في تطبيق eframe 0.36 على هيئة برنامج نقّال. مكتبات المشغّل القياسية (playerglobals) تُدمج في الملف التنفيذي نفسه، فلا يحتاج المستخدم عند التشغيل إلى Java ولا إلى أي ملف بيانات. Java لازمة على جهاز البناء وحده. والعائقان الجوهريان اثنان: Ruffle لا يشكّل النص العربي في الحقول الديناميكية، وتابع gotoFrame غير مُصدَّر من مكتبة ruffle_core.

Concrete plan:

(1) Dependencies. Use the three git crates at rev 07d3e430631333f80f754df01a968bdcd80d98c5: ruffle_core [default_font, audio, mp3, nellymoser], ruffle_render_wgpu with no features, and ruffle_render (for StageQuality only). Add eframe ~0.36.2 with default features, which brings egui-wgpu 0.36.2 and wgpu 30.x and matches Ruffle's Cargo.lock. Do not use ruffle_frontend_utils, because it pulls in reqwest/OpenSSL; write our own ~70-line cpal AudioBackend from frontend-utils/src/backends/audio.rs, keeping the MIT/Apache notice. Seed our Cargo.lock from Ruffle's lock to keep its tested versions.

(2) Device. Let eframe create the device. Use WgpuSetupCreateNew with: VALIDATION_INDIRECT_CALL removed from instance flags; a device_descriptor closure that copies Ruffle's private request_device (backend.rs:1216-1250); and, when the CPU setting is on, a native_adapter_selector that picks DeviceType::Cpu (WARP or lavapipe; WARP is enumerated by wgpu-hal 30). Then build one Arc<Descriptors> from eframe's RenderState clones (Descriptors::new(instance, adapter, device, queue)) and reuse it for every player rebuild. The 'requires hardware acceleration' message is not an obstacle. It only appears when request_adapter_and_device finds no adapter, and we never call that function. No native code checks DeviceType::Cpu. Switching between GPU and CPU takes effect after an app restart.

(3) Rendering. Implement a small RenderTarget (PreviewTarget: Rgba8Unorm, RENDER_ATTACHMENT|TEXTURE_BINDING, submit = queue.submit), modelled on desktop's MovieView. Do not use TextureTarget, which copies every frame into a MAP_READ buffer. Construct the backend with WgpuRenderBackend::new(descriptors, target). After every resize, re-register target.texture with egui (register_native_texture the first time, then update_egui_texture_from_wgpu_texture), detected through a generation counter. No sRGB conversion is needed: egui treats native textures as gamma-encoded, which is what Ruffle writes. Call player.render() inside App::ui so it submits before egui does.

(4) Player. Build it with PlayerBuilder: with_renderer, with_log (our LogBackend sending avm_trace/avm_warning to the Output panel), with_external_interface(HostEi), with_movie(SwfMovieData::from_data(bytes, "file:///jazari/preview.swf", None, None)), with_viewport_dimensions(physical w, physical h, pixels_per_point), ShowAll, Letterbox::On, LoadBehavior::Blocking, an explicit with_max_execution_duration, and with_autoplay(true), because the default leaves the player Suspended. Then call preload(ExecutionLimit::none()), run_frame() and render(). Drive playback with tick(FloatDuration::from_std(dt)), render only when needs_render(), and request_repaint_after(time_til_next_frame()). Pause with set_is_playing(false); step with run_frame() plus render(). Forward PlayerEvent mouse events in physical pixels relative to the image rect, keys/text only while the preview has focus, and map mouse_cursor() to egui's CursorIcon. Also install a tracing Layer on target 'ruffle_core' at WARN and above, because timeouts, halts, invalid bytecode and stubs never reach LogBackend.

(5) Goto. For exact gotoAndStop, preview builds should inject an ExternalInterface.addCallback("jz_goto") line into root frame 1, called through call_internal_interface followed by update(|_| {}). This avoids forking Ruffle. If the team accepts a one-line [patch] fork that exports GotoInfo/StopOrPlay, that is cleaner. Treat stepping with next_frame/prev_frame only as a fallback. My own view, labelled as opinion: as in the Flash 8 IDE, timeline scrubbing belongs to our own stage renderer, and Ruffle is best kept for 'Test Movie'.

(6) Arabic. Register Noto Sans Arabic (include_bytes!) through register_device_font. Use a merged Latin+Arabic font for the _sans default, because default fonts resolve to a single font. The publisher must shape and reorder Arabic itself into static DefineText glyph runs, since neither Ruffle nor Flash Player 8 shapes dynamic text.

(7) Build hygiene. Keep CARGO_HOME (C:\dev\rust\cargo) and the target dir on ASCII paths, put java on PATH (JAVA_HOME is ignored), and export these variables explicitly when building from the agent shell. Use panic=unwind plus catch_unwind around Ruffle calls. Ship Ruffle's MIT/Apache notices and the OFL text inside the portable bundle.

## Dependencies

- ruffle_core = { git = "https://github.com/ruffle-rs/ruffle", rev = "07d3e430631333f80f754df01a968bdcd80d98c5", features = ["default_font", "audio", "mp3", "nellymoser"] }  (absolute minimum: no features)
- ruffle_render_wgpu = { git = "https://github.com/ruffle-rs/ruffle", rev = "07d3e430631333f80f754df01a968bdcd80d98c5" }  (no features needed; it has no default features)
- ruffle_render = { git = "https://github.com/ruffle-rs/ruffle", rev = "07d3e430631333f80f754df01a968bdcd80d98c5" }  (only to name ruffle_render::quality::StageQuality)
- eframe = "~0.36.2"  (default features include wgpu; brings egui 0.36.2, egui-wgpu 0.36.2 and wgpu ^30.0, the same as Ruffle's lock; use eframe::egui_wgpu and eframe::wgpu)
- cpal = "0.16"  (optional, for our own AudioBackend; on Linux needs libasound2-dev to build and links libasound.so.2)
- anyhow = "1"
- tracing = "0.1" + tracing-subscriber = "0.3"  (optional Output-panel layer for Ruffle's tracing-only errors)
- Later, optional: ruffle_video_software from the same git/rev (h263, vp6, screenvideo) for FLV/embedded video
- Build-time only: a JDK with `java` on PATH on both Windows and Linux (Temurin 21 is installed here; JAVA_HOME is not read); MSVC Build Tools on Windows; pkg-config on Linux
- Avoid: ruffle_frontend_utils (always depends on reqwest; native-tls means OpenSSL on Linux) and ruffle_desktop

## Findings (verifier status in brackets; corrections override the claim)

- **F1** [confirmed] PlayerBuilder is the only construction path. PlayerBuilder::new() sets these defaults: autoplay=false, so the player stays RunState::Suspended and tick() does nothing; letterbox=Letterbox::Fullscreen; max_execution_duration is 15 s in release and u64::MAX in debug builds; viewport 550x400 at scale 1.0; LoadBehavior::Streaming; StageQuality::High; default_font=true (only with the default_font feature). build() returns Arc<Mutex<Player>>. Every backend left unset gets a null default: NullAudioBackend, NullLogBackend, NullNavigatorBackend, NullRenderer, MemoryStorageBackend, NullUiBackend, NullVideoBackend and DefaultLocaleBackend. The frame rate falls back to 12 fps until set_root_movie applies the SWF header rate. Builder methods: with_movie(SwfMovieData), with_renderer(impl RenderBackend), with_log, with_audio, with_navigator, with_storage(Box<dyn StorageBackend>), with_ui, with_video, with_locale, with_external_interface(Box<dyn ExternalInterfaceProvider>), with_fs_commands, with_viewport_dimensions(width: u32, height: u32, dpi_scale_factor: f64), with_scale_mode(StageScaleMode, force: bool), with_letterbox, with_quality, with_load_behavior, with_player_version(Option<u8>), with_frame_rate(Option<f64>), with_max_execution_duration(Duration), with_autoplay(bool), with_spoofed_url(Option<String>).
  - evidence: C:\dev\ref\ruffle\core\src\player.rs:2634-2680 (struct), 2688-2738 (new() defaults; autoplay 2703, letterbox 2711, timeout 2712-2716, viewport 2717-2719, Streaming 2720, High 2727, default_font 2736), 2742-2969 (with_* methods), 3039-3069 (null-backend defaults in build()), 3075 (12 fps), 3122-3126 (Suspended unless autoplay), 3221-3228 (movie set via set_root_movie). Re-exports: C:\dev\ref\ruffle\core\src\lib.rs:62-67 (PlayerEvent, LoadBehavior, Player, PlayerBuilder, FloatDuration, ViewportDimensions) and lib.rs:10 (StageAlign, StageDisplayState, StageScaleMode).
- **F2** [confirmed] An SWF can be loaded from in-memory bytes with a fake URL. In this revision the type is named SwfMovieData, not SwfMovie (SwfMovie<'gc> is now the GC wrapper). The signature is SwfMovieData::from_data(swf_data: &[u8], url: String, loader_url: Option<String>, load_bytes_info: Option<LoadBytesInfo>) -> Result<SwfMovieData, swf::error::Error>. It decompresses CWS itself. The URL scheme sets the security sandbox: file:// gives LocalWithFile, or LocalWithNetwork if the SWF8 FileAttributes UseNetwork bit is set; any other scheme gives Remote. set_root_movie is synchronous, so with_movie needs no navigator and no futures. Ruffle's exporter uses exactly this pattern.
  - evidence: C:\dev\ref\ruffle\core\common\src\tag_utils.rs:225-259 (from_data; decompress_swf at 232). Re-export at C:\dev\ref\ruffle\core\src\tag_utils.rs:6 (pub mod tag_utils, lib.rs:46). C:\dev\ref\ruffle\core\common\src\sandbox.rs:43-62 (SandboxType::infer). C:\dev\ref\ruffle\core\src\context.rs:359-397 (set_root_movie is sync). C:\dev\ref\ruffle\core\src\tag_utils.rs:103-113 (movie_from_path builds a file:// URL). C:\dev\ref\ruffle\exporter\src\exporter.rs:58-85.
- **F3** [confirmed] The minimal backend set for a preview is small. Ruffle's exporter uses only a WgpuRenderBackend<TextureTarget> plus DeterministicLocaleBackend. For our preview, two backends are needed: renderer (WgpuRenderBackend with our own target) and log (our LogBackend, for the Output panel). The ui backend can stay NullUiBackend, because Player tracks the cursor itself and exposes it through Player::mouse_cursor(). Storage can stay MemoryStorageBackend: SharedObjects are kept in RAM and nothing is written to disk, which suits a portable app. Video can stay NullVideoBackend for milestone 1; ruffle_video_software (h263/vp6/screenvideo) can be added later. Audio is either the null default or our own cpal backend. Ruffle desktop, by contrast, wires CpalAudioBackend, ExternalNavigatorBackend, a disk storage backend, DesktopUiBackend and software/external video.
  - evidence: C:\dev\ref\ruffle\exporter\src\exporter.rs:75-85. C:\dev\ref\ruffle\desktop\src\player.rs:132-141 (cpal), 213-232 (navigator), 234-258 (video), 274-276 (renderer), 302-340 (builder chain), 361-414 (set_default_font lists). C:\dev\ref\ruffle\core\src\player.rs:656-658 (mouse_cursor()), 1947-1948 (cursor is updated before calling ui.set_mouse_cursor). C:\dev\ref\ruffle\core\src\backend\ui.rs:213-289 (NullUiBackend). C:\dev\ref\ruffle\video\software\Cargo.toml (default = [h263, vp6, screenvideo]).
- **F4** [confirmed] Pitfall: NullNavigatorBackend::new() creates a NullExecutor (a futures LocalPool), keeps only its spawner and drops the executor. Spawned futures are therefore never polled, so loadMovie, loadVariables, XML.load and fetch_root_movie silently never finish. If the preview needs external files, build the navigator with NullNavigatorBackend::with_base_path(project_dir, &executor), keep the NullExecutor, and call executor.run() (run_until_stalled) on every tick. Root movies passed through with_movie are unaffected.
  - evidence: C:\dev\ref\ruffle\core\src\backend\navigator.rs:427-449 (NullNavigatorBackend::new / with_base_path), 355-370 (NullExecutor::new/spawner/run), 474-476 (spawn_future), 599-626 (fetch_path reads local files). C:\dev\ref\ruffle\core\src\player.rs:486-501 (fetch_root_movie depends on spawn_future).
- **F5** [confirmed] Do not depend on ruffle_frontend_utils just to get CpalAudioBackend. That crate always depends on reqwest, and its default feature native-tls means OpenSSL (libssl.so) on Linux, which breaks the 'no runtime deps' goal. CpalAudioBackend itself is about 70 lines (the whole file is 125): AudioMixer::new(channels, sample_rate), mixer.proxy().mix::<f32|i16>(buf) inside a cpal output stream, and impl AudioBackend via ruffle_core::impl_audio_mixer_backend!(mixer) with play/pause. Write our own copy (MIT/Apache attribution required) on cpal 0.16 with the ruffle_core feature 'audio'. On Linux, cpal links libasound.so.2.
  - evidence: C:\dev\ref\ruffle\frontend-utils\Cargo.toml:14 (default = ["native-tls"]), 15 (cpal feature), 27, 44 (non-optional reqwest). C:\dev\ref\ruffle\frontend-utils\src\backends\audio.rs:27-108. C:\dev\ref\ruffle\core\src\backend\audio.rs:24-27 (mixer behind feature "audio"). C:\dev\ref\ruffle\Cargo.lock:923-925 (cpal 0.16.0).
- **F6** [partly-wrong] Renderer construction: WgpuRenderBackend::<T: RenderTarget>::new(descriptors: Arc<Descriptors>, target: T) -> Result<Self, Error>, where Error = Box<dyn std::error::Error>. That error type is not Send or Sync, so convert it with map_err(|e| anyhow!(e.to_string())) as the exporter does. Descriptors::new(instance: wgpu::Instance, adapter: wgpu::Adapter, device: wgpu::Device, queue: wgpu::Queue) -> Descriptors takes owned clones; all of its fields are pub, and it caches pipelines per (msaa, format), so one Arc<Descriptors> should be shared across player rebuilds. RenderTarget trait: resize(&mut self, &wgpu::Device, u32, u32), format(), width(), height(), get_next_texture(&mut self) -> Option<Frame>, submit<I: IntoIterator<Item = wgpu::CommandBuffer>>(&self, &Device, &Queue, I, Frame) -> SubmissionIndex. RenderTargetFrame requires into_view() and view(). The stock TextureTarget (Rgba8Unorm, RENDER_ATTACHMENT|COPY_SRC|TEXTURE_BINDING, field `pub texture`) copies the whole frame into a MAP_READ buffer on every submit, which wastes work for a live preview. A custom target modelled on desktop's MovieView (Rgba8Unorm, RENDER_ATTACHMENT|TEXTURE_BINDING, submit = queue.submit) is the right choice. Player::set_viewport_dimensions calls target.resize, which recreates the texture, so the egui registration has to be refreshed after every resize. WgpuRenderBackend::for_offscreen builds its own Instance and Device, so it must not be used when sharing eframe's device.
  - evidence: C:\dev\ref\ruffle\render\wgpu\src\backend.rs:219-284 (new), 178-200 (for_offscreen), 385-395 (descriptors(), target(), device()), 429-458 (set_viewport_dimensions -> target.resize). C:\dev\ref\ruffle\render\wgpu\src\lib.rs:24 (type Error = Box<dyn std::error::Error>), 21 (pub use wgpu). C:\dev\ref\ruffle\render\wgpu\src\descriptors.rs:13-27, 35-65, 131-140. C:\dev\ref\ruffle\render\wgpu\src\target.rs:9-38 (traits), 191-272 (TextureTarget), 300-343 (submit copies to buffer). C:\dev\ref\ruffle\desktop\src\gui\movie.rs:154-277 (MovieView target).
  - CORRECTION: Not all Descriptors fields are pub. copy_pipeline (descriptors.rs:23) and pipelines (descriptors.rs:25) are private. The instance field is named `wgpu_instance`, not `instance` (descriptors.rs:14). Descriptors::new(instance, adapter, device, queue) -> Self is at 36-41 and copies device.limits() into `limits`. Also missing: the trait is `pub trait RenderTarget: Debug + 'static` (target.rs:15), and RenderBackend is implemented only for `T: RenderTarget + 'static` (backend.rs:427). WgpuRenderBackend::new returns Err if the target exceeds limits.max_texture_dimension_2d (backend.rs:220-231). set_viewport_dimensions silently clamps width/height to [1, max_texture_dimension_2d] before target.resize (backend.rs:429-446). The rest is confirmed: TextureTarget (Rgba8Unorm; RENDER_ATTACHMENT|COPY_SRC|TEXTURE_BINDING; copy to MAP_READ buffer at target.rs:300-343), for_offscreen (178-200), and `type Error = Box<dyn std::error::Error>` at lib.rs:24. TextureTarget.buffer is a pub Option (target.rs:196), so setting it to None skips the copy, but resize() recreates it (281-284), so a custom target is still the right call.
- **F7** [confirmed] Sharing eframe's device is exactly what Ruffle desktop does with its own egui: one Descriptors (Instance/Adapter/Device/Queue) feeds both WgpuRenderBackend<MovieView> and egui_wgpu::Renderer. Each frame, desktop runs egui, uploads egui textures and buffers, downcasts player.renderer_mut() to WgpuRenderBackend<MovieView>, then in a single render pass blits the movie texture and draws egui on top. In eframe 0.36.2 the shared objects are available as cc.wgpu_render_state: Option<egui_wgpu::RenderState> and Frame::wgpu_render_state(&self) -> Option<&egui_wgpu::RenderState>. RenderState is Clone and has pub fields adapter, instance, device, queue, target_format and renderer: Arc<epaint::mutex::RwLock<egui_wgpu::Renderer>>. Player::render() calls queue.submit synchronously, so calling it inside App::ui() puts Ruffle's frame on the shared queue before eframe's own submission.
  - evidence: C:\dev\ref\ruffle\desktop\src\gui\controller.rs:113 (Descriptors::new), 138-147 (egui_wgpu::Renderer::new on the same device), 277-285 (MovieView), 418-435 (egui textures/buffers), 450-457 (downcast to WgpuRenderBackend<MovieView>), 459-494 (one pass: movie blit, then egui; submit). https://docs.rs/crate/egui-wgpu/0.36.2/source/src/lib.rs (RenderState fields; use epaint::mutex::RwLock). https://docs.rs/crate/eframe/0.36.2/source/src/epi.rs (CreationContext.wgpu_render_state, Frame::wgpu_render_state, NativeOptions.wgpu_options). https://docs.rs/crate/eframe/0.36.2/source/src/lib.rs (re-exports egui_wgpu and egui_wgpu::wgpu).
- **F8** [confirmed] How egui displays the texture. egui_wgpu::Renderer::register_native_texture(&mut self, device: &wgpu::Device, texture: &wgpu::TextureView, texture_filter: wgpu::FilterMode) -> epaint::TextureId registers a view; update_egui_texture_from_wgpu_texture(&mut self, device, texture: &TextureView, texture_filter, id: TextureId) re-points an existing id after a resize; free_texture(&mut self, &TextureId) releases it. The documentation requires format Rgba8Unorm. egui.wgsl says 'We expect normal textures that are NOT sRGB-aware' and treats samples as gamma-encoded. Ruffle writes gamma-space Flash colours into a non-sRGB Rgba8Unorm target on purpose, so the Ruffle texture can be registered directly with no sRGB view or conversion. egui's default dithering adds slight noise to texture interpolation; this is cosmetic.
  - evidence: https://docs.rs/crate/egui-wgpu/0.36.2/source/src/renderer.rs (register_native_texture, register_native_texture_with_sampler_options, update_egui_texture_from_wgpu_texture, free_texture, RendererOptions). https://docs.rs/crate/egui-wgpu/0.36.2/source/src/egui.wgsl (fs_main_linear_framebuffer / fs_main_gamma_framebuffer; non-sRGB comment). C:\dev\ref\ruffle\render\wgpu\src\target.rs:69-72 (Flash colours and blending are done in sRGB space). C:\dev\ref\ruffle\desktop\src\gui\movie.rs:110-114, 181.
- **F9** [confirmed] The 'requires hardware acceleration' text does not reject software adapters. It is only the error message produced when instance.request_adapter() returns no adapter inside ruffle_render_wgpu::backend::request_adapter_and_device, which hard-codes force_fallback_adapter: false. The native Rust code (render, core, desktop) contains no DeviceType check of any kind. The only DeviceType::Cpu check is in the web front-end: it shows a 'hardware acceleration disabled' modal on mouseover when renderer_debug_info contains 'Adapter Device Type: Cpu', and it still runs. An embedder bypasses request_adapter_and_device entirely by passing any adapter to Descriptors::new, including WARP (DX12, software flag) or lavapipe/llvmpipe. With eframe, the WARP or lavapipe adapter is selected through WgpuSetupCreateNew.native_adapter_selector (an Arc<dyn Fn(&[wgpu::Adapter], Option<&wgpu::Surface>) -> Result<wgpu::Adapter, String>>) by matching get_info().device_type == wgpu::DeviceType::Cpu. Alternatively, use WgpuSetup::Existing(WgpuSetupExisting{instance, adapter, device, queue}) with our own request_adapter(... force_fallback_adapter: true ...). wgpu-hal 30.0.1's DXGI enumeration keeps the real WARP adapter, the one with DXGI_ADAPTER_FLAG_SOFTWARE.
  - evidence: C:\dev\ref\ruffle\render\wgpu\src\backend.rs:1188-1213 (message at 1203/1207; force_fallback_adapter: false at 1197), 470-479 (debug_info prints 'Adapter Device Type'). Repository-wide grep for DeviceType and force_fallback_adapter finds nothing else in Rust. C:\dev\ref\ruffle\web\packages\core\src\internal\player\inner.tsx:746-754 (web-only modal). https://docs.rs/crate/egui-wgpu/0.36.2/source/src/setup.rs (WgpuSetup::{CreateNew, Existing}, WgpuSetupCreateNew fields, NativeAdapterSelectorMethod, WgpuSetupCreateNew::without_display_handle(), default backends PRIMARY|GL). https://docs.rs/crate/wgpu-hal/30.0.1/source/src/auxil/dxgi/factory.rs (EnumAdapters1 loop; should_keep_adapter).
- **F10** [partly-wrong] Device requirements when eframe creates the device. Ruffle normally requests its own device: Limits::downlevel_webgl2_defaults().using_resolution(adapter).using_alignment(adapter), with max_uniform_buffer_binding_size and max_inter_stage_shader_variables raised to the adapter's values, max_color_attachments=4, and the optional features TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES | TEXTURE_COMPRESSION_BC | FLOAT32_FILTERABLE masked by adapter.features(). That request_device function is private. egui's default device descriptor uses Limits::default() (downlevel_webgl2 on GL) with max_texture_dimension_2d 8192 and no extra features. Ruffle picks MSAA counts from adapter.get_texture_format_features, not from the device (Low=1, Medium=2, High=4, High8x8=8, High16x16=16). So on a device created without TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES, quality Medium (2x) or 8x/16x can ask for sample counts the device was not granted. Fix: give eframe a device_descriptor closure that copies Ruffle's request_device, or restrict the preview to Low or High. Ruffle also removes wgpu::InstanceFlags::VALIDATION_INDIRECT_CALL when it creates the Instance, because otherwise device creation fails on GPUs without DYNAMIC_ARRAY_SIZE (wgpu#8799). Remove the same flag from eframe's instance_descriptor.flags.
  - evidence: C:\dev\ref\ruffle\render\wgpu\src\backend.rs:1215-1250 (private request_device), 42-69 (create_wgpu_instance removes VALIDATION_INDIRECT_CALL). C:\dev\ref\ruffle\render\wgpu\src\utils.rs:182-196 (supported_sample_count uses the adapter). C:\dev\ref\ruffle\render\src\quality.rs:65-76. https://docs.rs/crate/egui-wgpu/0.36.2/source/src/setup.rs (default device_descriptor: base limits plus max_texture_dimension_2d: 8192).
  - CORRECTION: Ruffle's optional features also include GpuProfiler::ALL_WGPU_TIMER_FEATURES (backend.rs:1235-1238); the report and the sketch leave it out. This is harmless while profile-with-tracy is off, because timer queries are then disabled (backend.rs:257-260). Everything else is confirmed: downlevel_webgl2_defaults().using_resolution().using_alignment(), the two raised limits, max_color_attachments=4 (1222-1231), MSAA from adapter.get_texture_format_features (utils.rs:182-196), and create_wgpu_instance removing VALIDATION_INDIRECT_CALL plus .with_env() (backend.rs:52-69). create_wgpu_instance is pub, so an embedder can call it directly. Sample counts: Best=4, and the *Linear variants are 8/16 (quality.rs:65-76). egui default device_descriptor: downlevel_webgl2 limits on GL, Limits::default() otherwise, max_texture_dimension_2d=8192, label 'egui wgpu device'.
- **F11** [confirmed] Driving playback. Player::tick(&mut self, dt: FloatDuration) returns immediately unless the player is playing. Otherwise it accumulates dt and calls run_frame() up to 5 times per tick (fewer if frames are slow), then updates timers, streams and audio. Player::time_til_next_frame() -> std::time::Duration is meant for ctx.request_repaint_after. set_is_playing(bool) pauses or resumes, and also pauses the audio stream. suspend_after_next_frame() runs exactly one more frame. run_frame() and render() are pub, so a manual step is run_frame() followed by render(). needs_render() -> bool lets the host skip render() when nothing changed, which matters on WARP. frame_rate() -> f64 is taken from the SWF header unless forced with with_frame_rate. preload(&mut ExecutionLimit) -> bool with ExecutionLimit::none() (ruffle_core::limits) finishes loading synchronously; the exporter calls it before run_frame. Desktop's loop computes dt = FloatDuration::from_std(now - last), calls tick, then renders only if needs_render.
  - evidence: C:\dev\ref\ruffle\core\src\player.rs:550-616 (tick), 516-527 (max 5 frames per tick), 618-639 (time_til_next_frame), 641-646, 930-950 (set_run_state, set_is_playing, suspend_after_next_frame), 952-954 (needs_render), 2024-2058 (preload), 2061-2096 (run_frame), 2099-2155 (render; submit_frame at 2151), 2180 (frame_rate). C:\dev\ref\ruffle\core\src\limits.rs:38 (ExecutionLimit::none). C:\dev\ref\ruffle\core\common\src\duration.rs:24-54 (FloatDuration::from_millis/from_secs/from_std/as_millis/to_std). C:\dev\ref\ruffle\desktop\src\app.rs:386-435. C:\dev\ref\ruffle\exporter\src\exporter.rs:108-130.
- **F12** [confirmed] Timeline goto is not part of the public API. MovieClip::goto_frame(self, &mut UpdateContext, GotoInfo { frame: u16, stop_or_play: StopOrPlay::{Stop, Play} }) is pub, but GotoInfo and StopOrPlay are re-exported only inside the crate-private display_object module. lib.rs declares `mod display_object;` and re-exports only StageAlign, StageDisplayState and StageScaleMode, so an external crate cannot construct a GotoInfo. What external code can call, through Player::update or mutate_with_update_context: ctx.stage.root_clip() -> Option<DisplayObject>, .as_movie_clip(), and then stop(ctx), play(), next_frame(ctx), prev_frame(ctx), current_frame() -> u16 and header_frames() -> u16. The exporter's force_root_clip_play uses this same pattern. Stepping to a frame with next_frame/prev_frame works, but every intermediate frame's DoAction gets queued and run, and backward gotos restart from frame 1, so stepping backwards is O(n^2). Exact gotoAndStop has two clean routes. (a) Preview builds inject `flash.external.ExternalInterface.addCallback("jz_goto", this, function(f){ this.gotoAndStop(f); })` into root frame 1 and the host calls Player::call_internal_interface("jz_goto", [Value::Number(n)]) followed by player.update(|_| {}) to run the queued frame actions. AVM1 addCallback returns false unless a provider was set with with_external_interface. (b) Carry a one-line Ruffle patch, `pub use display_object::{GotoInfo, StopOrPlay};`, through [patch]. Player::update runs queued actions, refreshes mouse state and runs GC; mutate_with_update_context does none of these.
  - evidence: C:\dev\ref\ruffle\core\src\display_object\movie_clip.rs:899-932 (goto_frame), 4685-4694 (GotoInfo, StopOrPlay), 840-877 (next_frame, play, prev_frame, stop), 964 (current_frame), 1123 (header_frames), 61 (type FrameNumber = u16), 1662 (rewinds restart at frame 1). C:\dev\ref\ruffle\core\src\display_object.rs:61 (crate-private re-export), 2986 (as_movie_clip). C:\dev\ref\ruffle\core\src\lib.rs:9-10 (private mod display_object). C:\dev\ref\ruffle\core\src\display_object\stage.rs:731 (root_clip). C:\dev\ref\ruffle\exporter\src\player_ext.rs:32-48. C:\dev\ref\ruffle\core\src\player.rs:2434-2456 (update), 2297 (mutate_with_update_context), 2523-2536 (call_internal_interface), 2936 (with_external_interface). C:\dev\ref\ruffle\core\src\avm1\globals\external_interface.rs:46-71 (addCallback needs available() and 3 args). C:\dev\ref\ruffle\core\src\external.rs:23-31 (Value), 355-361 (ExternalInterfaceProvider). C:\dev\ref\ruffle\core\src\debug_ui\display_object.rs:1204-1222 (Ruffle's own goto buttons, internal only).
- **F13** [confirmed] Input forwarding: Player::handle_event(&mut self, PlayerEvent) -> bool (true means handled). Variants: KeyDown{key: KeyDescriptor}, KeyUp{key}, MouseMove{x: f64, y: f64}, MouseUp{x, y, button: MouseButton}, MouseDown{x, y, button, index: Option<usize>}, MouseLeave, MouseWheel{delta: MouseWheelDelta::{Lines(f64), Pixels(f64)}}, GamepadButtonDown/Up, TextInput{codepoint: char}, TextControl{code: TextControlCode}, Ime(ImeEvent), FocusGained and FocusLost. Mouse x/y are physical pixels relative to the viewport origin; Ruffle maps them to stage coordinates with stage.inverse_view_matrix, so letterboxing and scaling are handled inside Ruffle. MouseButton has Unknown, Left, Right and Middle. KeyDescriptor { physical_key: PhysicalKey, logical_key: LogicalKey::{Unknown, Character(char), Named(NamedKey)}, key_location: KeyLocation }. Ruffle derives the AS key codes from it (physical or logical mapping). Desktop's winit-to-Ruffle key translation (util.rs:19 and 64) is the model for an egui::Key mapping. For buttons to show the hand cursor, read Player::mouse_cursor() (MouseCursor::{Arrow, Hand, IBeam, Grab}) and map it to egui::CursorIcon.
  - evidence: C:\dev\ref\ruffle\core\src\events.rs:10-51 (PlayerEvent), 55-58 (MouseWheelDelta), 459-485 (TextControlCode), 509 (ImeEvent), 658-663 (MouseButton), 888-892 (KeyDescriptor), 904-1067 (PhysicalKey: KeyA, Enter, Space, ArrowLeft...; LogicalKey), 1085+ (NamedKey), 1179-1184 (KeyLocation). C:\dev\ref\ruffle\core\src\player.rs:1051-1067 (handle_event), 1412-1415 (inverse_view_matrix * Point::from_pixels), 652 (set_mouse_in_stage). C:\dev\ref\ruffle\core\src\input.rs:238-257. C:\dev\ref\ruffle\desktop\src\app.rs:140-260 (winit to PlayerEvent), 401-420 (mouse-move coalescing). C:\dev\ref\ruffle\desktop\src\util.rs:19, 64. C:\dev\ref\ruffle\core\src\backend\ui.rs:191-211 (MouseCursor).
- **F14** [confirmed] Viewport units: ViewportDimensions { width: u32, height: u32, scale_factor: f64 }. Width and height are physical device pixels, which is also the size of the render-target texture, and scale_factor is the DPI factor. Player::set_viewport_dimensions(ViewportDimensions) resizes the target, rebuilds the stage matrices and sets needs_render = true. The stage uses scale_factor for the NoScale mode and for stageWidth/stageHeight. StageScaleMode has ExactFit, NoBorder, NoScale and ShowAll (the default). Letterbox has Off, Fullscreen (the builder default) and On; desktop uses Letterbox::On. For an egui panel of S points at pixels_per_point p, pass width = round(S.x*p), height = round(S.y*p), scale_factor = p, and only when one of these changes, because every call recreates the texture.
  - evidence: C:\dev\ref\ruffle\render\src\backend.rs:651-659. C:\dev\ref\ruffle\core\src\player.rs:980-993, 1029-1047. C:\dev\ref\ruffle\core\src\display_object\stage.rs:958-974 (StageScaleMode), about 493-543 (build_matrices uses viewport_size.scale_factor). C:\dev\ref\ruffle\core\src\config.rs:16-28 (Letterbox). C:\dev\ref\ruffle\desktop\src\player.rs:321.
- **F15** [confirmed] Output panel. Implement ruffle_core::backend::log::LogBackend { fn avm_trace(&self, message: &str); fn avm_warning(&self, message: &str); } and pass it with with_log. AVM1 trace() reaches avm_trace through UpdateContext::avm_trace, which converts \r to \n. Uncaught AS2 `throw` reaches avm_warning as 'Uncaught exception, ...'. Everything else goes only through the `tracing` crate: invalid bytecode, script timeouts ('A script in this movie has taken too long...') followed by the AVM being halted ('No more actions will be executed in this movie.'), avm_error!/avm_warn! such as 'GotoFrame failed', 'Encountered stub: ...', and missing fonts. Capturing these needs a tracing_subscriber Layer that forwards target prefix 'ruffle_core' at WARN and above to the panel. If no LogBackend is supplied, NullLogBackend sends traces to tracing target 'avm_trace' at INFO. No subscriber is required for Ruffle to run.
  - evidence: C:\dev\ref\ruffle\core\src\backend\log.rs:1-30. C:\dev\ref\ruffle\core\src\context.rs:472-478. C:\dev\ref\ruffle\core\src\avm1\activation.rs:2322 (trace action), 551-552 (ExecutionTimeout). C:\dev\ref\ruffle\core\src\avm1\runtime.rs:347-351 (halt), 652-680 (handle_error). C:\dev\ref\ruffle\core\src\avm1.rs:46-67 (avm_warn!/avm_error! use tracing). C:\dev\ref\ruffle\core\src\avm1\error.rs:9-10. C:\dev\ref\ruffle\core\src\stub.rs:142-147. C:\dev\ref\ruffle\desktop\src\main.rs:179-196 (desktop filter 'warn,ruffle=info,avm_trace=info').
- **F16** [confirmed] Java at build time. ruffle_core's build.rs always builds both playerglobals, and no cargo feature skips this. The AVM1 playerglobal is compiled in-process with Rascal (CompileOptions::with_swf_version(8)) and needs no Java. The AVM2 playerglobal calls asc::run_asc, which runs Command::new("java") with -classpath <CARGO_MANIFEST_DIR of tools/asc>/asc.jar and -outdir <OUT_DIR>. java is found on PATH only; JAVA_HOME is never read. When java is missing the error is 'Java could not be found on your computer...'. Both playerglobal SWFs are include_bytes!'d into the binary, so the shipped executable needs no Java and no data files. The non-ASCII risk: asc.jar lives under $CARGO_HOME/git/checkouts/... and OUT_DIR under CARGO_TARGET_DIR. If either path contains characters the Windows Java launcher cannot pass through (it converts arguments via the ANSI code page), the java step fails. The default CARGO_HOME here would be C:\Users\محمد وليد\.cargo, which has Arabic plus a space. On this machine the system ACP is 1256 (Arabic), so it might happen to survive, but other machines will not. On the build machine: user-level CARGO_HOME=C:\dev\rust\cargo and RUSTUP_HOME=C:\dev\rust\rustup are set, machine JAVA_HOME points to Temurin 21, and its bin is on the machine PATH, all ASCII. However, this agent's current shell has not inherited any of them (no cargo or java on PATH), so builds started from the harness must set them explicitly. Also, ruffle_core::PLAYERGLOBAL_ABC_PATH embeds the absolute OUT_DIR string in the binary (harmless, but it leaks the build path).
  - evidence: C:\dev\ref\ruffle\core\build.rs:3-23. C:\dev\ref\ruffle\core\build_playerglobal\src\avm1.rs:1-19 (Rascal). C:\dev\ref\ruffle\core\build_playerglobal\src\avm2.rs:53-118 (asc invocation). C:\dev\ref\ruffle\tools\asc\src\lib.rs:36-76 (Command::new("java"), asc.jar from CARGO_MANIFEST_DIR, NotFound message). C:\dev\ref\ruffle\core\src\avm1\globals.rs:82 and C:\dev\ref\ruffle\core\src\avm2\globals.rs:548 (include_bytes). C:\dev\ref\ruffle\core\src\lib.rs:79. C:\dev\ref\ruffle\README.md:59, 67. Registry reads: CARGO_HOME user=C:\dev\rust\cargo; JAVA_HOME machine=C:\Program Files\Eclipse Adoptium\jdk-21.0.12.101-hotspot\; ACP=1256, system locale ar-SY. `which java cargo` in this shell finds neither.
- **F17** [partly-wrong] Cargo features. ruffle_core has default = []. Its optional features are lzma (ZWS/SWF13+, not needed for CWS SWF8), avm_debug, tracy_avm, timeline_debug, mp3 (symphonia), aac (not used by SWF8), nellymoser ('Speech' compression), audio (dasp mixer; needed for any real audio backend), known_stubs, default_compatibility_rules (site-specific URL rewrites, irrelevant), egui (Ruffle's debug inspector, egui 0.36), jpegxr (FP11+), default_font (embeds a Latin/Greek/Cyrillic Noto Sans subset) and serde. ADPCM and PCM decoding are always compiled in. ruffle_render_wgpu has no default features (render_debug_labels, webgl, profile-with-tracy, tracy_images; 'clap' is only an optional dependency) and nothing is needed. ruffle_render is needed only to name ruffle_render::quality::StageQuality, because ruffle_core does not re-export it. The swf crate's own defaults (flate2, lzma) are always on through ruffle_core. Ruffle desktop uses: ruffle_core [audio, clap, mp3, aac, nellymoser, default_compatibility_rules, egui] plus lzma from its own default features; ruffle_render [clap]; ruffle_render_wgpu [clap]; optional ruffle_video_software and ruffle_video_external [openh264]; ruffle_frontend_utils [cpal, fs, navigator]. The exporter uses ruffle_core [default_font] and ruffle_render_wgpu [clap]. The minimum for AVM1 SWF8 preview is ruffle_core with no features plus ruffle_render_wgpu with no features. Recommended: ruffle_core [default_font, audio, mp3, nellymoser].
  - evidence: C:\dev\ref\ruffle\core\Cargo.toml:81-94. C:\dev\ref\ruffle\render\wgpu\Cargo.toml:41-45. C:\dev\ref\ruffle\render\Cargo.toml [features]. C:\dev\ref\ruffle\swf\Cargo.toml:27-28. C:\dev\ref\ruffle\desktop\Cargo.toml:23-28, 84-101. C:\dev\ref\ruffle\exporter\Cargo.toml:16-17. C:\dev\ref\ruffle\core\src\backend\audio\decoders.rs:3-17. C:\dev\ref\ruffle\core\src\compatibility_rules.rs:82-92.
  - CORRECTION: (a) 'ADPCM and PCM decoding are always compiled in' is wrong as stated. The whole `decoders` module is `#[cfg(feature = "audio")]` (core\src\backend\audio.rs:15-16). Within it, adpcm, g711 and pcm need no extra feature (decoders.rs:5-11), so G711 belongs on the list too. (b) clap is an implicit feature, not 'only an optional dependency', on both ruffle_core (Cargo.toml:52, which desktop enables) and ruffle_render_wgpu (Cargo.toml:18, which the exporter enables with features=["clap"]). (c) The other optional-dependency implicit features of core are omitted: lzma-rs, nellymoser-rs, dasp, symphonia, linkme, png. Confirmed: default = [] (core\Cargo.toml:81); render_wgpu features at 41-45; swf default = ["flate2", "lzma"] (swf\Cargo.toml:27) with core depending on swf without default-features=false (core\Cargo.toml:29), so LZMA swf decoding is always on; desktop core features at desktop\Cargo.toml:23, desktop defaults at 84 = [software_video, external_video, lzma, fontconfig, freetype].
- **F18** [confirmed] Fonts and Arabic. Without default_font and without registered fonts, device text is not drawn at all ('Known default device font not found... text will be missing'). With default_font, PlayerBuilder registers 'Noto Sans' from FALLBACK_DEVICE_FONT for every DefaultFont. That subset has no Arabic. Supplying our own font: Player::register_device_font(FontDefinition::FontFile { name: String, is_bold: bool, is_italic: bool, data: FontFileData::new(impl AsRef<[u8]> + 'static), index: u32 }), which accepts include_bytes! data directly, and Player::set_default_font(DefaultFont, Vec<String>). Library::default_font returns only the first font found (code comment: 'TODO: Return multiple fonts'), so _sans cannot fall back to an Arabic font. Per-glyph fallback (FontSet: main font, then fallbacks for each char) exists only for named device fonts, through UiBackend::sort_device_fonts. Options: register a merged Latin+Arabic font as the _sans default, or have the publisher name the Arabic font in TextFields. Critical: Ruffle's text layout does no OpenType shaping and no bidi. Glyphs are looked up one char at a time with ttf_parser glyph_index, and FontRenderer is char-based too. Arabic in dynamic/input TextFields therefore renders as isolated letters in logical (LTR) order, matching Flash Player 8's own limitation with embedded fonts. Static text (DefineText) is drawn by glyph index plus advance, so text shaped and reordered at publish time (e.g. with rustybuzz plus unicode-bidi in our publisher) displays correctly in the preview.
  - evidence: C:\dev\ref\ruffle\core\src\player.rs:85 (FALLBACK_DEVICE_FONT), 3154-3185 (default_font registration), 2567-2579 (register_device_font, set_default_font). C:\dev\ref\ruffle\core\assets\unicodes-file.txt (subset ranges: no 0600-06FF). C:\dev\ref\ruffle\core\src\backend\ui.rs:16-36 (FontDefinition), 147-159 (load_device_font, sort_device_fonts). C:\dev\ref\ruffle\core\src\font\font_face.rs:52-60 (FontFileData::new), 155-158 (glyph_index(character)). C:\dev\ref\ruffle\core\src\font\font_renderer.rs:5-19. C:\dev\ref\ruffle\core\src\library.rs:460-500 (break after first match), 560-592 (sort_device_fonts). C:\dev\ref\ruffle\core\src\font\font_set.rs:6-75. C:\dev\ref\ruffle\core\src\html\layout.rs:561-638. C:\dev\ref\ruffle\core\src\display_object\text.rs:180-187 (static text by glyph index). ruffle_core's Cargo.toml has no bidi or shaping crate; unicode-bidi is used only by desktop's GUI locale code (C:\dev\ref\ruffle\desktop\src\gui\locale.rs:61-67).
- **F19** [partly-wrong] Global and static requirements. No global initialisation is needed. Player state lives in a gc-arena owned by the Player; i18n uses an embedded static loader; playerglobals are compiled in. The Player is !Send (Rc and RefCell inside), so it must stay on the thread that created it; for eframe that means the UI thread, which is fine because App has no Send bound. Ruffle's workspace release profile sets panic = "abort", but dependency workspaces' profiles are not inherited, so our own profile decides. The exporter wraps render and capture in catch_unwind. Rust: Ruffle uses edition 2024 and let-chains, and dyn RenderBackend to dyn Any upcasting (used by desktop and the exporter) needs a recent stable toolchain; the installed stable-x86_64-pc-windows-msvc is fine.
  - evidence: C:\dev\ref\ruffle\Cargo.toml:47 (edition 2024), 164-165 (panic = abort). C:\dev\ref\ruffle\render\src\backend.rs:28 (RenderBackend: Any). C:\dev\ref\ruffle\exporter\src\exporter.rs:121-131 (catch_unwind), C:\dev\ref\ruffle\exporter\src\player_ext.rs:18-24 (downcast). C:\dev\ref\ruffle\core\src\player.rs:3077-3149 (Rc, RefCell, GcArena inside Player).
  - CORRECTION: The claim is right but the evidence lines are wrong. The !Send fields are in `pub struct Player` around player.rs:339-352 (Box<dyn RenderBackend> etc. without Send, gc_arena: Rc<RefCell<GcArena>> at 352), not in 3077-3149, which is builder code. Profiles: workspace [profile.dev] panic="unwind" (Cargo.toml:161-162), [profile.release] panic="abort" (164-165). RenderBackend: Any is at render\src\backend.rs:28. Upcasting &dyn RenderBackend to &dyn Any requires Rust >= 1.86. No rust-version or rust-toolchain file pins it; README says 'Latest stable'.
- **F20** [confirmed] Version alignment. The workspace asks for egui/egui-wgpu "0.36.1" and wgpu "30.0.1", but Ruffle's Cargo.lock actually resolves egui, egui-wgpu, egui-winit and egui_extras to 0.36.2, wgpu to 30.0.1, winit to 0.30.13, cpal to 0.16.0 and rascal to 0.3.6. Rascal 0.3.6 is the same version as our compiler crate, so cargo unifies them. eframe 0.36.2's default features include "wgpu", and it re-exports eframe::egui_wgpu and eframe::wgpu (wgpu ^30.0). eframe 0.37.0 was released on 2026-10-15 and still uses wgpu ^30.0, but Ruffle's optional egui debug UI is built on egui 0.36. Pinning ~0.36.2 keeps one egui. The eframe 0.36 App trait has `fn ui(&mut self, ui: &mut egui::Ui, frame: &mut Frame)` and `fn logic(&mut self, ctx, frame)`; there is no update().
  - evidence: C:\dev\ref\ruffle\Cargo.toml:61-67. C:\dev\ref\ruffle\Cargo.lock:1367-1369, 1387-1404, 1428-1430, 6958-6961, 7501-7503, 923-925, 4476-4479. https://crates.io/api/v1/crates/egui-wgpu/0.36.2/dependencies (wgpu ^30.0). https://crates.io/api/v1/crates/eframe/versions (0.37.0, 2026-10-15; 0.36.2, 2026-09-08). https://crates.io/api/v1/crates/egui-wgpu/0.37.0/dependencies (wgpu ^30.0). https://docs.rs/crate/eframe/0.36.2/source/Cargo.toml.orig (default features include wgpu). https://docs.rs/crate/eframe/0.36.2/source/src/epi.rs.

## Risks

- Arabic: Ruffle does no OpenType shaping and no bidi. Glyphs are looked up one char at a time (font_face.rs:155-158; font_renderer.rs:5-19), so Arabic in dynamic/input TextFields previews as isolated LTR letters. Only text shaped and reordered at publish time (DefineText glyph runs, text.rs:180-187) displays correctly. This is a content-level limitation the publisher must handle; the player cannot fix it.
- _sans/_serif default fonts resolve to only ONE font (library.rs:477-482, 'TODO: Return multiple fonts'). Registering Noto Sans Arabic beside the Latin Noto Sans subset does not give _sans an Arabic fallback. Fix: a merged Latin+Arabic font for the defaults, or explicitly named device fonts plus a custom UiBackend::sort_device_fonts.
- Timeline goto API is not exported: GotoInfo and StopOrPlay are crate-private (lib.rs:9-10; display_object.rs:61). Without a Ruffle patch or the ExternalInterface hook, scrubbing via next_frame/prev_frame runs every intermediate frame's script and costs O(n^2) going backwards (movie_clip.rs:1662).
- The Player runs on the egui UI thread and is !Send. An AS2 infinite loop freezes the whole IDE until max_execution_duration, which defaults to u64::MAX in debug builds (player.rs:2712-2716), so always set it explicitly. A worker-thread player would need double-buffered textures.
- Panics: with panic = "abort" in our profile, any Ruffle panic kills the authoring app and loses the user's work. Use unwind plus catch_unwind around tick/render (as exporter.rs:126 does) and rebuild the Player afterwards, because the Mutex is poisoned.
- Shared-device mismatch: if eframe's device lacks TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES, StageQuality::Medium (2x) or 8x/16x may request sample counts the device does not support, because Ruffle checks the adapter, not the device (utils.rs:182-196). Use the Ruffle-like device_descriptor or limit quality to Low/High.
- If eframe's instance keeps InstanceFlags::VALIDATION_INDIRECT_CALL, device creation can fail on some GPUs (Ruffle's own comment, backend.rs:42-51).
- The GPU/CPU switch needs an app restart, because eframe creates the single shared device at startup. A live switch would need a separate WARP device plus a CPU readback through TextureTarget::capture_frame.
- Git dependency: cargo clones the whole ruffle repo (the local .git is about 120 MB, tests about 213 MB) plus git deps such as regress, flash-lso and nellymoser. Our fresh Cargo.lock may resolve newer semver-compatible crates than Ruffle's tested lock. Seed our lock from C:\dev\ref\ruffle\Cargo.lock or pin wgpu =30.0.1 and egui/eframe =0.36.2. Alternatively use a [patch."https://github.com/ruffle-rs/ruffle"] that points at the local checkout (dev-only, absolute Windows path).
- Build env: ruffle_core's build.rs needs `java` on PATH on both OSes and every CI machine; no feature disables it. Paths passed to java (CARGO_HOME git checkout, CARGO_TARGET_DIR) should be ASCII. This agent's shell currently has neither cargo nor java on PATH, so builds started from the harness must export CARGO_HOME=C:\dev\rust\cargo, RUSTUP_HOME=C:\dev\rust\rustup and PATH explicitly.
- NullNavigatorBackend::new() never polls its executor (navigator.rs:434-441), so any loadMovie/loadVariables/XML in user content silently hangs unless we keep a NullExecutor and call run() every tick.
- Runtime-dependency creep on Linux: cpal links libasound.so.2; lavapipe needs mesa-vulkan-drivers for CPU mode. Both are present on stock Ubuntu 24.04 and Mint 22 desktops, but they are system libraries, not bundled.
- Licensing for the portable bundle: Ruffle is MIT OR Apache-2.0, so its notices must ship with the binary (e.g. an embedded About/licenses page); Noto fonts are OFL. Copying desktop or frontend-utils code (key mapping, cpal backend) carries the same notice obligation, even though our own code is Unlicense.
- eframe 0.37.0 is out (2026-10-15). Upgrading would split egui versions with Ruffle's optional egui debug UI (egui 0.36); keep ~0.36.2 until Ruffle moves.

## Open questions

- Exact goto for scrubbing: carry a one-line fork patch (`pub use display_object::{GotoInfo, StopOrPlay};` in core/src/lib.rs, via [patch]), or inject an ExternalInterface 'jz_goto' callback into preview builds only? Or keep scrubbing on our own authoring-stage renderer, as the Flash 8 IDE does, and use Ruffle only for 'Test Movie' (play/pause/step/rewind = rebuild the Player)?
- Should the preview emulate player version 8 (with_player_version(Some(8)), which changes some AVM1 quirks such as activation.rs:2493 and array.rs:149) or Ruffle's default 32? Should this be a setting?
- Does Rascal 0.3.6 compile the fully-qualified AS2 call flash.external.ExternalInterface.addCallback(...) to the expected GetVariable/GetMember chain? This needs a check in the Rascal research topic.
- Exact egui 0.36.2 Event field sets (PointerButton, MouseWheel phase/unit, Key physical_key) and whether set_focus_lock_filter is needed so Tab and arrow keys reach the preview. To be verified at first compile.
- Does eframe 0.36.2 add its own display handle to WgpuSetupCreateNew on Linux/Wayland (relevant to the GL/llvmpipe fallback), or should Linux CPU mode prefer Vulkan lavapipe? Not verified.
- Ruffle's frame cost on WARP/lavapipe at typical preview sizes (550x400 to 1100x800). Should CPU mode force StageQuality::Low? To be measured CPU-only on this machine.
- eframe's 'persistence' feature writes to the OS config dir (eframe::storage_dir). Do we disable it and store settings next to the executable, per the portable-app constraint?

## Verifier: problems in the code sketch

- Compile error: the block labelled preview.rs uses Arc (Arc<Descriptors>, Arc<Mutex<Player>>, Arc::new) and egui/egui_wgpu/wgpu, but its `use` list imports only std::{any::Any, sync::{Mutex, mpsc, atomic::..}, time::..}. `use std::sync::Arc;` and `use eframe::{egui, egui_wgpu, wgpu};` exist only in the separate eframe-setup block. Add both imports to preview.rs.
- Input TextFields cannot be edited. Backspace, Delete, Enter, arrow keys, Copy/Paste and select-all reach Ruffle only as KeyDown/KeyUp and never as PlayerEvent::TextControl { code: TextControlCode::Backspace | Delete | Enter | MoveLeft | ... } (events.rs:459-485). Desktop sends both: winit_to_ruffle_text_control at desktop\src\util.rs:19-30 plus the KeyDown. egui::Event::Copy, Cut and Paste(String) are also dropped.
- Hover state goes stale. MouseLeave is sent only on egui::Event::PointerGone, which fires when the pointer leaves the window. PointerMoved outside resp.rect is filtered out, so moving from the preview to another egui panel leaves Flash buttons in the over state and the hand cursor stuck. Track hover per frame and send PlayerEvent::MouseLeave when the pointer exits resp.rect; Player also has set_mouse_in_stage (player.rs:652).
- Wrong wheel magnitude. egui::Event::MouseWheel carries a `unit` (Point/Line/Page), but the sketch always sends MouseWheelDelta::Lines(delta.y). Map Point to MouseWheelDelta::Pixels (events.rs:55-58).
- Focus arrives late and keys can be stolen. request_focus() runs on resp.clicked(), which fires on release, so the first key presses are lost. egui's focus navigation also consumes Tab and arrow keys unless a focus-lock filter is set with ui.memory_mut(|m| m.set_focus_lock_filter(id, egui::EventFilter{tab: true, horizontal_arrows: true, vertical_arrows: true, escape: false, ..})). Flash games commonly use arrow keys.
- Wasted CPU while paused. request_repaint_after(p.time_til_next_frame()) runs every frame; when the player is suspended, time_til_next_frame() still returns one frame duration (player.rs:624-639), so eframe keeps repainting at the SWF frame rate. On WARP/lavapipe this is real CPU load. Only request a repaint when p.is_playing() or needs_render() is true.
- goto_and_stop_stepwise is worse than described. Each prev_frame() is a full rewind goto (movie_clip.rs:855-865 -> goto_frame -> restart from frame 1, 1662ff), so every backward step re-runs from frame 1, and each step queues that frame's DoAction. A script on an intermediate frame (gotoAndPlay, stop) can redirect the playhead in the middle of the loop. The loop guard catches 'no movement' but not redirection. Keep this strictly as a fallback.
- No panic isolation. The recommendation (and F19) says to wrap tick/render in catch_unwind and rebuild the Player, but the sketch calls player.lock().unwrap() everywhere. After any Ruffle panic the Mutex is poisoned and every later lock().unwrap() panics again inside App::ui, taking down the IDE.
- CPU mode can block startup. When no DeviceType::Cpu adapter exists (Linux without mesa-vulkan-drivers; llvmpipe GL may not appear with WgpuSetupCreateNew::without_display_handle), native_adapter_selector returns Err and eframe fails to create the renderer, so the app never opens. Fall back to the default selection and show a warning instead of returning Err.
- Device descriptor omits a feature. The device_descriptor copy leaves out GpuProfiler::ALL_WGPU_TIMER_FEATURES from Ruffle's optional_features (backend.rs:1235-1238). This is harmless without profile-with-tracy, but the comment calls it a copy of Ruffle's request_device, which it is not exactly.
- Halted AVM is not detected. After one script timeout or invalid bytecode, AVM1 is permanently halted for that Player (runtime.rs:347-352, 676-680). The sketch has no way to notice this, because those messages go only to tracing, not to LogBackend, and it never rebuilds the player. The preview silently stops running scripts.

## Verifier: missed points

- Portable settings: eframe 0.36.2 NativeOptions has `persistence_path: Option<std::path::PathBuf>`, and `persistence` is not a default feature. Pointing it at a path next to the executable answers the report's open question about settings location.
- 'No download at runtime': do not copy desktop's video path. ruffle_video_external with the openh264 feature calls OpenH264Codec::load(cache_directory/'video') (desktop\src\player.rs:234-250), which fetches the Cisco binary at runtime. For a portable app use only ruffle_video_software (h263/vp6/screenvideo), which is enough for SWF8/FLV-era video.
- ruffle_render_wgpu::backend::create_wgpu_instance(backends, backend_options, display) (backend.rs:52-69) and request_adapter_and_device (backend.rs:1188) are pub. An embedder can build eframe's device with WgpuSetup::Existing from Ruffle's exact instance config instead of hand-copying flags. For CPU mode it must still call instance.request_adapter with force_fallback_adapter: true itself, because request_adapter_and_device hard-codes false (1197).
- One timeout kills scripts for the rest of the session. AVM1 halt is permanent per Player (runtime.rs:347-352). The preview needs a 'restart preview' action, or an automatic Player rebuild when the tracing layer sees 'No more actions will be executed in this movie.'. Desktop itself defaults max_execution_duration to Duration::MAX (desktop\src\player.rs:319), so copying desktop settings gives no timeout at all.
- Neither egui nor Ruffle rejects software adapters. egui-wgpu 0.36.2 only logs 'Software rasterizer detected - loss of performance expected.' for DeviceType::Cpu. The GPU/CPU switch therefore needs no workaround beyond adapter selection, but it should surface the chosen adapter (Player renderer debug_info prints 'Adapter Device Type', backend.rs:470-479) in the settings or About panel so users can see which mode is active.
- Linux runtime system libraries outside Ruffle: winit/eframe dlopen libxkbcommon, libwayland-client or libX11/libxcb, and libvulkan or libEGL/libGL; cpal links libasound.so.2. All are present on stock Ubuntu 24.04 and Mint 22 desktops, but the 'no runtime dependencies' claim should say 'only standard desktop system libraries', and a CI smoke test on a clean image is advisable. Ruffle's README Linux list (README.md:63-67) includes libssl-dev only because of frontend-utils/desktop.
- Large viewports are silently clamped. set_viewport_dimensions clamps to descriptors.limits.max_texture_dimension_2d (backend.rs:429-446), and WgpuRenderBackend::new errors above it (220-231). With egui's default device descriptor that limit is 8192, but on downlevel GL (llvmpipe) it can be lower. The preview should cap its physical size itself so the egui image size and Ruffle's texture size stay consistent.
- Text entry for Arabic in input fields: besides TextControl mapping, egui delivers IME composition as egui::Event::Ime. Ruffle has PlayerEvent::Ime(ImeEvent) (events.rs:509), and desktop forwards winit IME. Forwarding egui IME events is needed for input methods; even then, Arabic input renders unshaped LTR, as F18 says.

## Code sketch (researcher; check against corrections above)
```rust
# ---------------- Cargo.toml (app crate) ----------------
[dependencies]
ruffle_core        = { git = "https://github.com/ruffle-rs/ruffle", rev = "07d3e430631333f80f754df01a968bdcd80d98c5", features = ["default_font", "audio", "mp3", "nellymoser"] }
ruffle_render      = { git = "https://github.com/ruffle-rs/ruffle", rev = "07d3e430631333f80f754df01a968bdcd80d98c5" }   # only for StageQuality
ruffle_render_wgpu = { git = "https://github.com/ruffle-rs/ruffle", rev = "07d3e430631333f80f754df01a968bdcd80d98c5" }
eframe = "~0.36.2"          # default features include "wgpu"; gives eframe::egui_wgpu + eframe::wgpu (30.0.x)
cpal   = "0.16"             # optional: own ~70-line AudioBackend (copy of frontend-utils/src/backends/audio.rs, MIT/Apache notice)
anyhow = "1"
tracing = "0.1"
tracing-subscriber = "0.3"  # optional Layer that forwards target "ruffle_core" WARN+ to the Output panel
# NOT: ruffle_frontend_utils (pulls reqwest + native-tls/OpenSSL), ruffle_desktop.
# Build env (both OSes): `java` on PATH (JAVA_HOME is ignored); CARGO_HOME and target dir on ASCII paths.

// ---------------- eframe setup (GPU/CPU switch applied at startup) ----------------
use std::sync::Arc;
use eframe::{egui, egui_wgpu, wgpu};

pub fn native_options(cpu_mode: bool) -> eframe::NativeOptions {
    use egui_wgpu::{WgpuConfiguration, WgpuSetup, WgpuSetupCreateNew};
    let mut setup = WgpuSetupCreateNew::without_display_handle();
    // same as ruffle_render_wgpu::backend::create_wgpu_instance (backend.rs:61-68)
    setup.instance_descriptor.flags =
        setup.instance_descriptor.flags.difference(wgpu::InstanceFlags::VALIDATION_INDIRECT_CALL);
    if cpu_mode {
        // WARP (DX12, Windows) / lavapipe (Vulkan) / llvmpipe (GL) report DeviceType::Cpu
        setup.native_adapter_selector = Some(Arc::new(|adapters: &[wgpu::Adapter], _s: Option<&wgpu::Surface<'_>>| {
            adapters.iter().find(|a| a.get_info().device_type == wgpu::DeviceType::Cpu).cloned()
                .ok_or_else(|| "no software adapter (WARP/lavapipe/llvmpipe)".to_owned())
        }));
    }
    // copy of ruffle's private request_device (backend.rs:1216-1250) so Ruffle gets what it expects
    setup.device_descriptor = Arc::new(|adapter: &wgpu::Adapter| {
        let mut limits = wgpu::Limits::downlevel_webgl2_defaults()
            .using_resolution(adapter.limits())
            .using_alignment(adapter.limits());
        limits.max_uniform_buffer_binding_size = adapter.limits().max_uniform_buffer_binding_size;
        limits.max_inter_stage_shader_variables = adapter.limits().max_inter_stage_shader_variables;
        limits.max_color_attachments = 4;
        let optional = wgpu::Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES
            | wgpu::Features::TEXTURE_COMPRESSION_BC
            | wgpu::Features::FLOAT32_FILTERABLE;
        wgpu::DeviceDescriptor {
            label: Some("jazari"),
            required_features: optional & adapter.features(),
            required_limits: limits,
            memory_hints: Default::default(),
            trace: wgpu::Trace::Off,
            experimental_features: wgpu::ExperimentalFeatures::disabled(),
        }
    });
    eframe::NativeOptions {
        wgpu_options: WgpuConfiguration { wgpu_setup: WgpuSetup::CreateNew(setup), ..Default::default() },
        ..Default::default()
    }
}

// ---------------- preview.rs ----------------
use std::{any::Any, sync::{Mutex, mpsc, atomic::{AtomicU64, Ordering}}, time::{Duration, Instant}};
use anyhow::anyhow;
use ruffle_core::{
    Player, PlayerBuilder, PlayerEvent, FloatDuration, LoadBehavior, StageScaleMode, ViewportDimensions,
    backend::{log::LogBackend, ui::{FontDefinition, MouseCursor}},
    config::Letterbox, context::UpdateContext,
    events::{KeyDescriptor, MouseButton, MouseWheelDelta},
    external::{ExternalInterfaceProvider, Value as ExtValue},
    font::{DefaultFont, FontFileData}, limits::ExecutionLimit, tag_utils::SwfMovieData,
};
use ruffle_render::quality::StageQuality;
use ruffle_render_wgpu::{backend::WgpuRenderBackend, descriptors::Descriptors,
                         target::{RenderTarget, RenderTargetFrame}};

static TEX_GEN: AtomicU64 = AtomicU64::new(1);
static NOTO_SANS_ARABIC: &[u8] = include_bytes!("../assets/fonts/NotoSansArabic-Regular.ttf");

/// Render target egui can sample directly (modelled on desktop/src/gui/movie.rs MovieView).
#[derive(Debug)]
pub struct PreviewTarget { texture: wgpu::Texture, generation: u64 }
impl PreviewTarget {
    fn new(device: &wgpu::Device, w: u32, h: u32) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("jazari-preview"),
            size: wgpu::Extent3d { width: w.max(1), height: h.max(1), depth_or_array_layers: 1 },
            mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,   // egui native textures: Rgba8Unorm, gamma values
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        Self { texture, generation: TEX_GEN.fetch_add(1, Ordering::Relaxed) }
    }
}
#[derive(Debug)] pub struct PreviewFrame(wgpu::TextureView);
impl RenderTargetFrame for PreviewFrame {
    fn into_view(self) -> wgpu::TextureView { self.0 }
    fn view(&self) -> &wgpu::TextureView { &self.0 }
}
impl RenderTarget for PreviewTarget {
    type Frame = PreviewFrame;
    fn resize(&mut self, device: &wgpu::Device, w: u32, h: u32) { *self = PreviewTarget::new(device, w, h); }
    fn format(&self) -> wgpu::TextureFormat { self.texture.format() }
    fn width(&self) -> u32 { self.texture.width() }
    fn height(&self) -> u32 { self.texture.height() }
    fn get_next_texture(&mut self) -> Option<PreviewFrame> {
        Some(PreviewFrame(self.texture.create_view(&Default::default())))
    }
    fn submit<I: IntoIterator<Item = wgpu::CommandBuffer>>(&self, _d: &wgpu::Device, q: &wgpu::Queue,
        cbs: I, _f: PreviewFrame) -> wgpu::SubmissionIndex { q.submit(cbs) }
}

pub enum OutputLine { Trace(String), Warning(String) }
struct OutputLog(mpsc::Sender<OutputLine>);
impl LogBackend for OutputLog {
    fn avm_trace(&self, m: &str)   { let _ = self.0.send(OutputLine::Trace(m.to_owned())); }
    fn avm_warning(&self, m: &str) { let _ = self.0.send(OutputLine::Warning(m.to_owned())); }
}

/// Needed so AS2 ExternalInterface.addCallback succeeds (avm1/globals/external_interface.rs:51).
struct HostEi;
impl ExternalInterfaceProvider for HostEi {
    fn call_method(&self, _c: &mut UpdateContext<'_>, _n: &str, _a: &[ExtValue]) -> ExtValue { ExtValue::Undefined }
    fn on_callback_available(&self, _n: &str) {}
    fn get_id(&self) -> Option<String> { None }
}

pub struct PreviewPlayer {
    descriptors: Arc<Descriptors>,             // shared across rebuilds (pipeline cache)
    player: Option<Arc<Mutex<Player>>>,
    tex: Option<(egui::TextureId, u64)>,
    size_px: (u32, u32), ppp: f64, last_tick: Instant,
    log_tx: mpsc::Sender<OutputLine>, pub log_rx: mpsc::Receiver<OutputLine>,
}

impl PreviewPlayer {
    /// From cc.wgpu_render_state (CreationContext) or frame.wgpu_render_state().
    pub fn new(rs: &egui_wgpu::RenderState) -> Self {
        let descriptors = Arc::new(Descriptors::new(
            rs.instance.clone(), rs.adapter.clone(), rs.device.clone(), rs.queue.clone()));
        let (log_tx, log_rx) = mpsc::channel();
        Self { descriptors, player: None, tex: None, size_px: (550, 400), ppp: 1.0,
               last_tick: Instant::now(), log_tx, log_rx }
    }

    pub fn load_bytes(&mut self, swf: &[u8]) -> anyhow::Result<()> {
        let movie = SwfMovieData::from_data(swf, "file:///jazari/preview.swf".into(), None, None)?;
        let (w, h) = self.size_px;
        let renderer = WgpuRenderBackend::new(self.descriptors.clone(),
                                              PreviewTarget::new(&self.descriptors.device, w, h))
            .map_err(|e| anyhow!(e.to_string()))?;            // Box<dyn Error> is !Send
        let player = PlayerBuilder::new()
            .with_renderer(renderer)
            .with_log(OutputLog(self.log_tx.clone()))
            .with_external_interface(Box::new(HostEi))
            // .with_audio(JazariCpalAudio::new()?)           // optional
            .with_movie(movie)
            .with_viewport_dimensions(w, h, self.ppp)
            .with_scale_mode(StageScaleMode::ShowAll, false)
            .with_letterbox(Letterbox::On)
            .with_quality(StageQuality::High)                 // Low in CPU mode
            .with_player_version(Some(8))                     // Flash-8-like AVM1 quirks (opinion)
            .with_load_behavior(LoadBehavior::Blocking)
            .with_max_execution_duration(Duration::from_secs(15))
            .with_autoplay(true)                              // default false = Suspended
            .build();
        {
            let mut p = player.lock().unwrap();
            p.register_device_font(FontDefinition::FontFile {
                name: "Noto Sans Arabic".into(), is_bold: false, is_italic: false,
                data: FontFileData::new(NOTO_SANS_ARABIC), index: 0,
            });
            // _sans returns only ONE font (library.rs:477-482): use a merged Latin+Arabic font for it
            // p.set_default_font(DefaultFont::Sans, vec!["Jazari Sans".into()]);
            p.preload(&mut ExecutionLimit::none());
            p.run_frame();
            p.render();
        }
        self.player = Some(player);
        self.last_tick = Instant::now();
        Ok(())
    }

    pub fn set_playing(&self, on: bool) { if let Some(p) = &self.player { p.lock().unwrap().set_is_playing(on) } }
    pub fn step(&self) { if let Some(p) = &self.player { let mut p = p.lock().unwrap(); p.run_frame(); p.render(); } }

    /// Exact gotoAndStop. Preview builds inject into root frame 1:
    ///   flash.external.ExternalInterface.addCallback("jz_goto", this, function(f){ this.gotoAndStop(f); });
    pub fn goto_and_stop(&self, frame: u16) {
        if let Some(p) = &self.player {
            let mut p = p.lock().unwrap();
            p.set_is_playing(false);
            p.call_internal_interface("jz_goto", [ExtValue::Number(frame as f64)]);
            p.update(|_ctx| {});   // runs the queued DoAction of the target frame
            p.render();
        }
    }

    /// Fallback without injection (GotoInfo is not exported): runs intermediate frame scripts; backward = O(n^2).
    pub fn goto_and_stop_stepwise(&self, frame: u16) {
        if let Some(p) = &self.player {
            let mut p = p.lock().unwrap();
            p.set_is_playing(false);
            p.update(|ctx| {
                if let Some(mc) = ctx.stage.root_clip().and_then(|r| r.as_movie_clip()) {
                    mc.stop(ctx);
                    let target = frame.clamp(1, mc.header_frames().max(1));
                    loop {
                        let cur = mc.current_frame();
                        if cur < target { mc.next_frame(ctx) } else if cur > target { mc.prev_frame(ctx) } else { break }
                        if mc.current_frame() == cur { break }       // frame not loaded / cannot move
                    }
                }
            });
            p.render();
        }
    }

    /// Call from eframe::App::ui(&mut self, ui, frame).
    pub fn ui(&mut self, ui: &mut egui::Ui, frame: &eframe::Frame) {
        let Some(player) = self.player.clone() else { return };
        let rs = frame.wgpu_render_state().expect("wgpu renderer");
        let ppp = ui.ctx().pixels_per_point() as f64;
        let avail = ui.available_size();
        let size_px = (((avail.x as f64) * ppp).round().max(1.0) as u32,
                       ((avail.y as f64) * ppp).round().max(1.0) as u32);
        let mut p = player.lock().unwrap();

        if size_px != self.size_px || ppp != self.ppp {               // every call recreates the texture
            self.size_px = size_px; self.ppp = ppp;
            p.set_viewport_dimensions(ViewportDimensions { width: size_px.0, height: size_px.1, scale_factor: ppp });
        }
        let now = Instant::now();
        p.tick(FloatDuration::from_std(now - self.last_tick));
        self.last_tick = now;
        if p.needs_render() { p.render(); }                           // submits before eframe's submit

        // (re)bind the Ruffle target to an egui TextureId when Ruffle recreated it
        let backend = <dyn Any>::downcast_ref::<WgpuRenderBackend<PreviewTarget>>(p.renderer()).expect("renderer type");
        let target = backend.target();
        if self.tex.map(|(_, g)| g) != Some(target.generation) {
            let view = target.texture.create_view(&Default::default());
            let mut r = rs.renderer.write();                          // epaint::mutex::RwLock
            let id = match self.tex {
                Some((id, _)) => { r.update_egui_texture_from_wgpu_texture(&rs.device, &view, wgpu::FilterMode::Linear, id); id }
                None => r.register_native_texture(&rs.device, &view, wgpu::FilterMode::Linear),
            };
            self.tex = Some((id, target.generation));
        }
        let (tex_id, _) = self.tex.unwrap();

        let resp = ui.add(egui::Image::new(egui::load::SizedTexture::new(tex_id, avail))
            .sense(egui::Sense::click_and_drag()));
        if resp.clicked() { resp.request_focus(); }
        let origin = resp.rect.min;
        let to_px = |pos: egui::Pos2| (((pos.x - origin.x) as f64) * ppp, ((pos.y - origin.y) as f64) * ppp);
        for ev in ui.input(|i| i.events.clone()) {
            match ev {
                egui::Event::PointerMoved(pos) if resp.rect.contains(pos) => {
                    let (x, y) = to_px(pos); p.handle_event(PlayerEvent::MouseMove { x, y });
                }
                egui::Event::PointerButton { pos, button, pressed, .. } if pressed == false || resp.rect.contains(pos) => {
                    let (x, y) = to_px(pos);
                    let button = match button {
                        egui::PointerButton::Primary => MouseButton::Left,
                        egui::PointerButton::Secondary => MouseButton::Right,
                        egui::PointerButton::Middle => MouseButton::Middle,
                        _ => MouseButton::Unknown,
                    };
                    p.handle_event(if pressed { PlayerEvent::MouseDown { x, y, button, index: None } }
                                   else { PlayerEvent::MouseUp { x, y, button } });
                }
                egui::Event::PointerGone => { p.handle_event(PlayerEvent::MouseLeave); }
                egui::Event::MouseWheel { delta, .. } if resp.hovered() => {
                    p.handle_event(PlayerEvent::MouseWheel { delta: MouseWheelDelta::Lines(delta.y as f64) });
                }
                egui::Event::Text(s) if resp.has_focus() => {
                    for c in s.chars() { p.handle_event(PlayerEvent::TextInput { codepoint: c }); }
                }
                egui::Event::Key { key, pressed, .. } if resp.has_focus() => {
                    if let Some(kd) = egui_key_to_ruffle(key) {
                        p.handle_event(if pressed { PlayerEvent::KeyDown { key: kd } } else { PlayerEvent::KeyUp { key: kd } });
                    }
                }
                _ => {}
            }
        }
        if resp.hovered() {
            ui.ctx().set_cursor_icon(match p.mouse_cursor() {
                MouseCursor::Hand => egui::CursorIcon::PointingHand,
                MouseCursor::IBeam => egui::CursorIcon::Text,
                MouseCursor::Grab => egui::CursorIcon::Grabbing,
                MouseCursor::Arrow => egui::CursorIcon::Default,
            });
        }
        if p.needs_render() { p.render(); }                           // button states changed by input
        ui.ctx().request_repaint_after(p.time_til_next_frame());
    }
}

/// egui::Key -> KeyDescriptor { physical_key: PhysicalKey::KeyA.., logical_key: LogicalKey::Character('a') |
/// LogicalKey::Named(NamedKey::Enter).., key_location: KeyLocation::Standard }; model: desktop/src/util.rs:64.
fn egui_key_to_ruffle(_key: egui::Key) -> Option<KeyDescriptor> { todo!() }
```
