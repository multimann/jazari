# Research: app-shell (verified)

## Recommendation

المبدأ الحاكم في «البرنامج النقّال» ألا يحتاج الملف التنفيذي إلا ما يملكه نظام المستخدم أصلًا، وأن يجد إعداداته حيث وُضع. وعلى هذا المبدأ تُبنى التوصية:

١) الإصدارات: eframe 0.36.2، وهو ما يترجم عليه Ruffle فعلًا عند الرقم المثبّت لأن ملف القفل يحلّ egui-wgpu إلى 0.36.2، ومعه wgpu 30.0.1 وegui_dock 0.21.1 وrfd 0.17.2 (default-features=false، الميزتان xdg-portal وwayland) وdirs 6.0. تثبيت السلسلة 0.36 مفروض بسبب egui_dock، فلا يُرقّى إلى eframe 0.37. ويُتحقق من نسخة wgpu الواحدة بالأمر `cargo tree -i wgpu`، وأي ازدواج يظهر خطأَ ترجمة عند تمرير `rs.device` إلى `Descriptors::new`.

٢) جهاز واحد مشترك: يتولى eframe إنشاء الجهاز عبر `WgpuSetup::CreateNew`، لا `Existing`، ونمرّر إليه ثلاثة أمور. أولها instance flags خالية من `VALIDATION_INDIRECT_CALL`، كما يفعل Ruffle. وثانيها `native_adapter_selector` يصفّي بحسب النمط (`DeviceType::Cpu` لوضع CPU، وغيره لوضع GPU) ويتحقق من `is_surface_supported`. وثالثها `device_descriptor` يطلب `adapter.limits()` مع ميزات Ruffle الاختيارية مقاطعةً بما يدعمه المحوّل. ثم نبني من `cc.wgpu_render_state` كائن `ruffle_render_wgpu::descriptors::Descriptors` مرة واحدة لكل جهاز، ويُعاد استعماله لكل Player.

٣) مفتاح GPU/CPU: يُقرأ من settings.toml قبل `run_native`. ويُختار على ويندوز في وضع CPU الطرف `Backends::DX12` وحده، لأن WARP لا يأتي إلا عبره، مع `Dx12Compiler::Fxc`. وعلى لينكس `VULKAN | GL`، فيأتي lavapipe أولًا ثم llvmpipe. إن أعادت `run_native` القيمة `Err(eframe::Error::Wgpu(_))` في وضع GPU أُعيد الاستدعاء في وضع CPU، اعتمادًا على `run_and_return=true` وحلقة الأحداث thread_local. وإن فشل وضع CPU عُرضت رسالة rfd تقترح على لينكس تثبيت mesa-vulkan-drivers. والتراجع باتجاه واحد فقط، من GPU إلى CPU. ولحماية هذا الجهاز يوضع `JAZARI_FORCE_CPU=1` في `[env]` داخل `C:\dev\rust\cargo\config.toml` المحلي، لا في المستودع.

٤) المعاينة: هدف رسم مخصّص (`impl RenderTarget`) على نمط `MovieView` في Ruffle، نسيجه Rgba8Unorm بالاستعمالين RENDER_ATTACHMENT|TEXTURE_BINDING. يُسجَّل بـ `register_native_texture` مرة واحدة، ثم يُحدَّث بـ `update_egui_texture_from_wgpu_texture` عند تغيّر الحجم فقط. ويُستدعى `player.render()` داخل `App::ui` قبل رسم egui؛ والطابور واحد، فالترتيب مضمون بلا مزامنة.

٥) الألواح: `DockState<Tab>` بالألواح Stage وTimeline وLibrary وProperties وActions وOutput، ومعها Preview، ويُعرض بـ `DockArea::show_inside(ui, ..)` داخل `fn ui` الجديدة في eframe 0.36. ويُحفظ التخطيط عبر eframe::set_value، وتُضاف ميزة persistence مع `persistence_path` يشير إلى ملف داخل مجلد البيانات النقّال. ويُضمَّن خط عربي، وتبقى العناوين المختلطة موضع اختبار لغياب ترتيب bidi.

٦) الإعدادات النقّالة، بهذا الترتيب: متغير JAZARI_HOME. ثم مجلد `JazariData` بجوار الملف التنفيذي إن أمكنت الكتابة فيه (افتراضيًا لـ exe وtar.gz). وفي AppImage يُعتمد مجلد `$APPIMAGE` بدل `current_exe` لأن هذا الأخير داخل تركيب للقراءة فقط، ولا يكون نقّالًا إلا بملف علامة أو مجلد موجود. ثم ملاذ أخير هو `dirs::config_dir()/Al-Jazari`، وهو يحترم تلقائيًا `XDG_CONFIG_HOME` الذي يضبطه وضع `<name>.AppImage.config`.

٧) ويندوز: ملف exe واحد بـ `+crt-static`، وقد أثبت Ruffle جدواه في إصداراته. ويضاف `/STACK:4000000` و`#![windows_subsystem = "windows"]`. الأيقونة بـ winresource اختيارية، ولا حاجة إلى manifest. ويُتحقق بالأمر `dumpbin /dependents` من خلوّ الملف من VCRUNTIME140.

٨) لينكس: glibc ديناميكية، والواجهات الرسومية كلها تُحمَّل بـ dlopen من النظام، فلا يُحزم Mesa ولا مكتبات X وWayland. حزم البناء: build-essential pkg-config libasound2-dev default-jre-headless. وأبسط توزيع tar.gz مسطّح يحتوي الملف التنفيذي وملف `org.aljazari.Jazari.desktop` المطابق لمعرّف Wayland. وإن أُريد AppImage فبوقت التشغيل type2 الساكن. ويُتحقق بالأمر `readelf -d` من أن الاعتماد الصلب مقصور على libc وlibasound.

والتمييز واجب هنا: ما سبق في البنود ١ إلى ٤ و٧ قائم على المصدر مباشرة. أما سياسة «النقّال افتراضيًا» واختيار tar.gz على AppImage فرأي في تصميم المنتج يُعرض على المستخدم ليقرّه.

## Dependencies

- eframe = { version = "0.36.2", features = ["persistence"] }   # يجرّ egui/egui-wgpu/egui-winit 0.36.2 وwinit 0.30.13
- egui_dock = { version = "0.21.1", features = ["serde"] }   # MIT، egui ^0.36، MSRV 1.95
- wgpu = "30.0.1"   # تثبيت صريح؛ يتوحّد مع ruffle_render_wgpu (تصدّره بـ pub use)
- rfd = { version = "0.17.2", default-features = false, features = ["xdg-portal", "wayland"] }   # مطابق لـ Ruffle؛ بلا GTK، وlibdbus عبر dlopen
- dirs = "6.0"   # النسخة نفسها في شجرة Ruffle (توجد 7.0.0 أيضًا)
- serde = { version = "1.0", features = ["derive"] }
- toml = "1.1"   # ملف settings.toml النقّال
- log = "0.4"
- ruffle_core = { git = "https://github.com/ruffle-rs/ruffle", rev = "07d3e430631333f80f754df01a968bdcd80d98c5" }   # ميزاته تُحدَّد في موضوع Ruffle
- ruffle_render_wgpu = { git = "https://github.com/ruffle-rs/ruffle", rev = "07d3e430631333f80f754df01a968bdcd80d98c5" }
- [target.'cfg(target_os = "linux")'.dependencies] winit = { version = "0.30.13", default-features = false }   # لأجل EventLoopBuilderExtX11::with_x11 فقط
- [build-dependencies] winresource = "0.1.34"   # اختياري: أيقونة exe؛ يحتاج rc.exe من Windows SDK عند البناء
- إن احتيج صوت Ruffle عبر ruffle_frontend_utils: ruffle_frontend_utils = { git = ..., rev = ..., default-features = false, features = ["cpal"] }   # بلا native-tls، فلا ربط بـ libssl
- .cargo/config.toml: [target.x86_64-pc-windows-msvc] rustflags = ["-C", "target-feature=+crt-static"]
- build.rs: cargo:rustc-link-arg-bins=/STACK:4000000 لهدف windows-msvc (كما يفعل Ruffle المكتبي)
- Ubuntu 24.04 / Mint 22 للبناء: sudo apt install build-essential pkg-config libasound2-dev default-jre-headless git curl
- Ubuntu لاختبار وضع CPU أو CI بلا شاشة: sudo apt install mesa-vulkan-drivers libegl-mesa0 xvfb
- Windows للبناء: VS 2022 Build Tools (C++ وWindows SDK) وJava على PATH (كلاهما مثبّت حسب C:\dev\install-log.txt)

## Findings (verifier status in brackets; corrections override the claim)

- **F1** [confirmed] الإصدار: eframe 0.36.1 و0.36.2 كلاهما يعتمد egui-wgpu ^0.36.x، وهذا يعتمد wgpu ^30.0 (default-features=false مع std وwgsl)، فيتوحّد كلاهما مع wgpu 30.0.1 في نسخة واحدة (وهي أعلى نسخة مستقرة الآن). والأدق أن Ruffle عند الرقم المثبّت يطلب في ملف البيان "0.36.1"، لكن ملف القفل عنده يحلّها إلى 0.36.2 (egui وegui-wgpu وegui-winit وepaint وegui_extras). لذا التوصية eframe 0.36.2. صدر eframe 0.37.0 في 2026-10-15، وهو أيضًا على wgpu ^30، لكنه يجرّ egui 0.37، وegui_dock 0.21.1 لا يقبل إلا egui ^0.36؛ فالسلسلة 0.36 مفروضة.
  - evidence: https://crates.io/api/v1/crates/eframe/0.36.2/dependencies و/0.36.1/dependencies (egui-wgpu ^0.36.x)؛ https://crates.io/api/v1/crates/egui-wgpu/0.36.2/dependencies (wgpu ^30.0)؛ https://github.com/emilk/egui/blob/0.36.2/Cargo.toml#L28 (version 0.36.2) و#L160 (wgpu 30.0)؛ C:\dev\ref\ruffle\Cargo.toml:62-67؛ C:\dev\ref\ruffle\Cargo.lock (egui-wgpu 0.36.2, wgpu/wgpu-core/wgpu-hal/wgpu-types/naga 30.0.1, winit 0.30.13)؛ https://crates.io/api/v1/crates/egui_dock/0.21.1/dependencies (egui ^0.36)
- **F2** [confirmed] واجهة egui-wgpu 0.36.2: النوع `enum WgpuSetup { CreateNew(WgpuSetupCreateNew), Existing(WgpuSetupExisting) }`. حقول `WgpuSetupCreateNew` كلها عامة: `instance_descriptor: wgpu::InstanceDescriptor`، و`display_handle: Option<Box<dyn EguiDisplayHandle>>`، و`power_preference`، و`native_adapter_selector: Option<NativeAdapterSelectorMethod>`، و`device_descriptor: Arc<dyn Fn(&wgpu::Adapter) -> wgpu::DeviceDescriptor<'static> + Send + Sync>`. والنوع `NativeAdapterSelectorMethod = Arc<dyn Fn(&[wgpu::Adapter], Option<&wgpu::Surface<'_>>) -> Result<wgpu::Adapter, String> + Send + Sync>`. أما `WgpuSetupExisting { instance, adapter, device, queue }`. القيم الافتراضية: backends = from_env أو PRIMARY|GL، وflags = from_build_config().with_env()، وHighPerformance، وLimits::default مع max_texture_dimension_2d=8192. لا يوجد خيار force_fallback_adapter، إذ تثبّته egui-wgpu على false، وتنص تعليقاتها على أن المحدِّد native_adapter_selector يغني عنه.
  - evidence: https://github.com/emilk/egui/blob/0.36.2/crates/egui-wgpu/src/setup.rs#L41-L62 (WgpuSetup)، #L150-L158 (NativeAdapterSelectorMethod)، #L172-L208 (WgpuSetupCreateNew)، #L240-L279 (القيم الافتراضية)، #L322-L331 (WgpuSetupExisting)؛ https://github.com/emilk/egui/blob/0.36.2/crates/egui-wgpu/src/lib.rs#L147-L157 (force_fallback_adapter: false مع التعليل). نسخ محلية في C:\Users\A04E~1\AppData\Local\Temp\claude\C--Users-----------CluadeCowork---------------\b1f4a133-7216-4e29-a076-c5e0c76642d5\scratchpad\src\egui
- **F3** [partly-wrong] مسار الإنشاء: تعدِّد `RenderState::create` المحوّلات بحسب `instance_descriptor.backends`، ثم تمرّرها إلى المحدِّد مع `Some(&surface)`. إن أعاد المحدِّد Err(String) صار `WgpuError::CustomNativeAdapterSelectionError`، ثم `eframe::Error::Wgpu`، فتعيده `run_native` خطأً عاديًا بلا انهيار. أما مع `WgpuSetup::Existing` فلا تُصفّى المحوّلات بحسب توافقها مع سطح النافذة، و`configure_surface` تستدعي `.expect("The surface isn't supported by this adapter")`؛ فالأسلم CreateNew مع محدِّد مخصّص وواصف جهاز مخصّص، لا Existing. ويحقن eframe بنفسه `OwnedDisplayHandle` إذا تُرك display_handle فارغًا.
  - evidence: https://github.com/emilk/egui/blob/0.36.2/crates/egui-wgpu/src/lib.rs#L45-L65 (WgpuError)، #L204-L267 (create: المحدِّد L237-L249، وExisting L261-L266)؛ https://github.com/emilk/egui/blob/0.36.2/crates/egui-wgpu/src/winit.rs#L120-L130 (expect) و#L246؛ https://github.com/emilk/egui/blob/0.36.2/crates/eframe/src/native/wgpu_integration.rs#L217-L224 (حقن مقبض العرض) و#L258-L261؛ https://github.com/emilk/egui/blob/0.36.2/crates/eframe/src/lib.rs#L528-L530 (Error::Wgpu)
  - CORRECTION: الصحيح منه: المحدِّد يتلقى available_adapters المعدودة بحسب instance_descriptor.backends مع Some(surface) (lib.rs:214-222 و238-240)، وخطؤه يُغلَّف في WgpuError::CustomNativeAdapterSelectionError (lib.rs:50-51)، ثم يصل إلى eframe::Error::Wgpu عبر From (eframe/src/lib.rs:568-572)، وeframe يحقن owned_display_handle (wgpu_integration.rs:217-224). أما القول بأن Existing مع محوّل غير متوافق ينتهي إلى expect في configure_surface فغير دقيق: إذ تستدعي RenderState::create قبل ذلك s.get_capabilities(&adapter).formats (lib.rs:271-278)، وwgpu يعيد قدرات فارغة للسطح غير المتوافق (https://github.com/gfx-rs/wgpu/blob/v30.0.1/wgpu/src/api/surface.rs#L52-L55)، فتفشل preferred_framebuffer_format بالخطأ WgpuError::NoSurfaceFormatsAvailable، وهو خطأ عادي لا انهيار. والمحدِّد نفسه لا يصفّي بحسب السطح، فتلك مسؤوليته هو لا مسؤولية Existing. وتبقى التوصية بـ CreateNew صائبة.
- **F4** [confirmed] العارض البرمجي: في wgpu-core يعني force_fallback_adapter الإبقاء على `DeviceType::Cpu` وحده. على ويندوز، WARP («Microsoft Basic Render Driver») يحمل `DXGI_ADAPTER_FLAG_SOFTWARE` فيُصنَّف `DeviceType::Cpu`، ويُبقيه مرشّح DXGI ويحذف نسخة WARP التي تدّعي أنها مدمجة. ولا يُتاح WARP إلا عبر DX12، إذ لا واجهة D3D11 في wgpu 30؛ فوضع CPU على ويندوز يلزمه `Backends::DX12`. على لينكس، lavapipe/llvmpipe عبر Vulkan من نوع `PhysicalDeviceType::CPU` فيصير Cpu، وllvmpipe عبر GL يُعرَّف Cpu من نص اسم المعرِّف. فيكفي محدِّد يصفّي `get_info().device_type == DeviceType::Cpu` ويتحقق من `is_surface_supported`.
  - evidence: https://github.com/gfx-rs/wgpu/blob/v30.0.1/wgpu-core/src/instance.rs#L511-L525 (تصفية Cpu) و#L527-L546 (تصفية السطح) و#L571-L603 (الترتيب)؛ https://github.com/gfx-rs/wgpu/blob/v30.0.1/wgpu-hal/src/dx12/adapter.rs#L171 و#L191-L204؛ https://github.com/gfx-rs/wgpu/blob/v30.0.1/wgpu-hal/src/auxil/dxgi/factory.rs#L35-L47 و#L83-L110؛ https://github.com/gfx-rs/wgpu/blob/v30.0.1/wgpu-hal/src/vulkan/adapter.rs#L2242؛ https://github.com/gfx-rs/wgpu/blob/v30.0.1/wgpu-hal/src/gles/adapter.rs#L144-L155؛ https://github.com/gfx-rs/wgpu/blob/v30.0.1/wgpu/Cargo.toml#L37-L46 (الافتراضي: dx12, metal, gles, vulkan, wgsl)
- **F5** [partly-wrong] متطلبات Ruffle من الجهاز: (أ) تنشئ Ruffle المثيل مع حذف `InstanceFlags::VALIDATION_INDIRECT_CALL`، لأن تحقّق wgpu يفشل في إنشاء الجهاز على بعض البطاقات (wgpu#8799). و`from_build_config()` في wgpu 30 تُرجع هذه الراية حتى في نسخة release، فلا بد من حذفها في instance_descriptor الذي نسلّمه لـ eframe. (ب) تطلب Ruffle حدودًا مأخوذة من المحوّل، وميزات اختيارية هي TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES وTEXTURE_COMPRESSION_BC وFLOAT32_FILTERABLE مقاطعةً مع ما يدعمه المحوّل. (ج) `Descriptors::new(instance, adapter, device, queue)` تقبل أي جهاز وتقرأ `device.limits()`، فمشاركة جهاز eframe ممكنة مباشرة. وتصدّر ruffle_render_wgpu الحزمة wgpu (`pub use wgpu`)، فأي اختلاف في النسخة يظهر خطأَ ترجمة عند تمرير `rs.device`.
  - evidence: C:\dev\ref\ruffle\render\wgpu\src\backend.rs:42-69 (create_wgpu_instance)، :1188-1213 (request_adapter_and_device، وفيه force_fallback_adapter: false)، :1216-1250 (request_device)؛ C:\dev\ref\ruffle\render\wgpu\src\descriptors.rs:35-60؛ C:\dev\ref\ruffle\render\wgpu\src\lib.rs:21؛ https://github.com/gfx-rs/wgpu/blob/v30.0.1/wgpu-types/src/instance.rs#L263-L274
  - CORRECTION: البند (أ) صحيح في جوهره، لكن create_wgpu_instance تستعمل InstanceFlags::default() لا from_build_config() (C:\dev\ref\ruffle\render\wgpu\src\backend.rs:62-64)، والنتيجة واحدة لأن default() تساوي from_build_config() (wgpu-types instance.rs:244-248). البند (ب) غير دقيق: Ruffle لا تطلب adapter.limits() كاملة، بل تبدأ من Limits::downlevel_webgl2_defaults()، ثم تطبّق using_resolution وusing_alignment، وترفع max_uniform_buffer_binding_size وmax_inter_stage_shader_variables، وتضع max_color_attachments = 4 (backend.rs:1218-1232). والميزات الاختيارية عندها تشمل أيضًا GpuProfiler::ALL_WGPU_TIMER_FEATURES (backend.rs:1234-1239). البند (ج) صحيح: Descriptors::new(instance, adapter, device, queue) في C:\dev\ref\ruffle\render\wgpu\src\descriptors.rs:35-41، و`pub use wgpu;` في lib.rs:21.
- **F6** [confirmed] الوصول إلى RenderState: `CreationContext::wgpu_render_state: Option<egui_wgpu::RenderState>` حقل عام، و`Frame::wgpu_render_state(&self) -> Option<&egui_wgpu::RenderState>`. حقول RenderState العامة: `adapter`، و`available_adapters: Vec<wgpu::Adapter>` على المنصات الأصلية، و`instance`، و`device`، و`queue`، و`target_format`، و`renderer: Arc<RwLock<egui_wgpu::Renderer>>` (من epaint::mutex، ويُكتب فيه بـ `.write()`)، و`surface_config`. صيغة الإطار المفضّلة Rgba8Unorm أو Bgra8Unorm بلا sRGB، أي في فضاء غاما، وهو فضاء Ruffle نفسه. وفي eframe 0.36 تغيّر توقيع الدالة إلى `fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame)`، ولم تعد `update` قائمة.
  - evidence: https://github.com/emilk/egui/blob/0.36.2/crates/eframe/src/epi.rs#L78-L84، #L797-L805، #L152-L182 (App::ui)؛ https://github.com/emilk/egui/blob/0.36.2/crates/egui-wgpu/src/lib.rs#L105-L137 (RenderState) و#L422-L438 (preferred_framebuffer_format)
- **F7** [confirmed] تسجيل نسيج المعاينة: `Renderer::register_native_texture(&mut self, device: &wgpu::Device, texture: &wgpu::TextureView, texture_filter: wgpu::FilterMode) -> epaint::TextureId` تشترط Rgba8Unorm، وكل استدعاء يحجز معرّفًا جديدًا (next_user_texture_id += 1) ومجموعة ربط جديدة؛ فتسجيلها في كل إطار تسريب. الصواب أن تُسجَّل مرة واحدة، ثم تُستدعى `update_egui_texture_from_wgpu_texture(device, view, filter, id)` عند تغيّر الحجم فقط، لأن Ruffle تنشئ نسيجًا جديدًا في `RenderTarget::resize`. ومجموعة الربط تشير إلى النسيج نفسه، فكل ما ترسمه Ruffle فيه يظهر تلقائيًا. أما `TextureTarget` الجاهز في Ruffle فينسخ النسيج كل إطار إلى مخزن MAP_READ، وهذا هدر هنا؛ فالأنسب هدف مخصّص على نمط `MovieView` في Ruffle المكتبي (Rgba8Unorm مع RENDER_ATTACHMENT|TEXTURE_BINDING). وما دام الطابور واحدًا (wgpu::Queue) فإن `player.render()` داخل `ui()` يرسَل قبل رسم egui، فالترتيب مضمون بلا مزامنة.
  - evidence: https://github.com/emilk/egui/blob/0.36.2/crates/egui-wgpu/src/renderer.rs#L769-L813 و#L852-L863 و#L755-L759؛ C:\dev\ref\ruffle\render\wgpu\src\target.rs:15-38 (RenderTarget)، :212-263 (TextureTarget::new)، :300-343 (النسخ إلى المخزن)؛ C:\dev\ref\ruffle\desktop\src\gui\movie.rs:154-262 (MovieView)؛ C:\dev\ref\ruffle\desktop\src\gui\controller.rs:450-494
- **F8** [confirmed] egui_dock: النسختان 0.21.0 و0.21.1 تعتمدان egui ^0.36، برخصة MIT، وrust-version 1.95 (المترجم المحلي 1.99.1). يلزم في `TabViewer` النوع `type Tab` والدوال `fn id(&mut self, tab: &mut Self::Tab) -> egui::Id` و`fn title(..) -> egui::WidgetText` و`fn ui(&mut self, ui: &mut egui::Ui, tab: &mut Self::Tab)`، ومن الاختياري `is_closeable(&self, &Tab) -> bool`. وفيه أيضًا `DockState::new(Vec<Tab>)`، و`main_surface_mut()`، و`split_above/below/left/right(parent: NodeIndex, fraction: f32, tabs: Vec<Tab>) -> [NodeIndex; 2]` حيث fraction حصة العقدة القديمة. يُعرض بـ `DockArea::new(&mut dock).style(Style::from_egui(ui.style().as_ref())).show_inside(ui, &mut viewer)`، والنصوص قابلة للتعريب عبر `DockState::with_translations(Translations{..})`، وميزة `serde` تحفظ التخطيط.
  - evidence: https://raw.githubusercontent.com/anhosh/egui_dock/main/Cargo.toml (version 0.21.1، والمستودع انتقل من Adanos020 إلى anhosh، ولا وسوم فيه)؛ src/widgets/tab_viewer.rs:6-21 و42-44؛ src/widgets/dock_area/mod.rs:51؛ src/widgets/dock_area/show/mod.rs:23؛ src/dock_state/mod.rs:114،123،134؛ src/dock_state/tree/mod.rs:359-441
- **F9** [confirmed] لوحات egui 0.36: `egui::Panel::top(id).show(ui, ..)` و`CentralPanel::default().show(ui, ..)`؛ أما `show_inside` فمهجورة بعد أن صار اسمها show. شريط القوائم: `egui::MenuBar::new().ui(ui, ..)`. الصورة: `egui::Image::from_texture(egui::load::SizedTexture::new(id, size))`. وفي `ViewportBuilder` الدوال `with_app_id` و`with_drag_and_drop(bool)` و`with_icon`، ويحوّل `eframe::icon_data::from_png_bytes` صورة PNG مضمّنة إلى أيقونة.
  - evidence: https://github.com/emilk/egui/blob/0.36.2/crates/egui/src/containers/panel.rs#L249-L274 و#L422-L428 و#L1212-L1218؛ crates/egui/src/containers/menu.rs#L217-L257؛ crates/egui/src/widgets/image.rs#L102؛ crates/egui/src/load.rs#L456-L465؛ crates/egui/src/viewport.rs#L434 و#L600 و#L645؛ crates/eframe/src/icon_data.rs#L24
- **F10** [confirmed] حوارات الملفات: يستعمل Ruffle `rfd 0.17.2` مع default-features=false والميزتين ["wayland","xdg-portal"]. وفي 0.17.2 لم تعد xdg-portal تجرّ zbus أو ashpd، بل pollster وحدها، وتخاطب D-Bus بتحميل `libdbus-1.so.3` ديناميكيًا وقت التشغيل (dlopen)؛ فإن فشلت البوابة لجأت إلى برنامج `zenity`. لا اعتماد ربط عند البناء، ولا حاجة إلى GTK إلا مع الميزة `gtk3`. وحوار الرسائل `MessageDialog` على لينكس يمر عبر zenity. والدالة `set_parent<W: HasWindowHandle + HasDisplayHandle + ?Sized>` تقبل `&eframe::Frame` لأن Frame يحقّق السمتين.
  - evidence: C:\dev\ref\ruffle\Cargo.toml:96؛ https://github.com/PolyMeilex/rfd/blob/0.17.2/Cargo.toml#L13-L19 و#L74-L90؛ https://github.com/PolyMeilex/rfd/blob/0.17.2/src/backend/xdg_desktop_portal/portal/ffi.rs#L199 و#L215-L224؛ .../portal/libdbus.rs#L75-L79؛ src/backend/xdg_desktop_portal.rs#L98-L113 (لجوء zenity) و#L291-L303 (الرسائل)؛ src/file_dialog.rs#L96؛ https://github.com/emilk/egui/blob/0.36.2/crates/eframe/src/epi.rs#L699-L715
- **F11** [confirmed] السحب والإفلات: في egui 0.36 صار الحقل `RawInput::dropped_files: Vec<DroppedFileHandle>`، والنوع `DroppedFileHandle = Arc<dyn DroppedFile + Send + Sync>` يوفّر `.path() -> &Path` و`.bytes()`. والمرجع الحاكم winit 0.30.13: لا ينفّذ إفلات الملفات إلا على X11 (الملف x11/dnd.rs)، ولا وحدة data-device في واجهة Wayland. فعلى جلسة Wayland أصلية لا تصل الملفات المفلَتة. والعلاج إعداد اختياري «فرض X11/XWayland» عبر `NativeOptions::event_loop_builder` مع `winit::platform::x11::EventLoopBuilderExtX11::with_x11()`. على ويندوز يعمل الإفلات ما لم يُعطَّل بـ with_drag_and_drop(false).
  - evidence: https://github.com/emilk/egui/blob/0.36.2/crates/egui/src/data/input/dropped_file.rs#L10-L44؛ crates/egui/src/data/input/raw_input.rs#L63-L81؛ شجرة https://github.com/rust-windowing/winit/tree/v0.30.13/src/platform_impl/linux (الملف الوحيد x11/dnd.rs، ولا مقابل له في wayland/)؛ https://github.com/rust-windowing/winit/blob/v0.30.13/src/platform/x11.rs#L114-L127؛ https://github.com/emilk/egui/blob/0.36.2/crates/eframe/src/epi.rs#L14 و#L28-L34 و#L344-L351
- **F12** [confirmed] Wayland وX11: الميزتان `wayland` و`x11` كلتاهما ضمن ميزات eframe الافتراضية مع `winit/default`، فيُبنى الدعمان معًا ويُختار أحدهما وقت التشغيل: Wayland إن وُجد WAYLAND_DISPLAY أو WAYLAND_SOCKET، وإلا X11 إن وُجد DISPLAY. ويُستعمل الوسيط app_name في run_native معرّفَ تطبيق على Wayland، فيجب أن يطابق اسم ملف .desktop. ولا يعمل `centered` على Wayland. وعند تعطيل ميزات eframe الافتراضية يجب أن تُضاف يدويًا "wayland" و"x11" و"winit/default" (وفيها wayland-dlopen).
  - evidence: https://github.com/emilk/egui/blob/0.36.2/crates/eframe/Cargo.toml#L27-L37 و#L69-L75 و#L106-L114؛ https://github.com/rust-windowing/winit/blob/v0.30.13/src/platform_impl/linux/mod.rs#L734-L767؛ https://github.com/emilk/egui/blob/0.36.2/crates/eframe/src/lib.rs#L241-L250؛ crates/eframe/src/epi.rs#L362-L367
- **F13** [confirmed] اعتماديات التشغيل على لينكس: الواجهات الرسومية كلها تُحمَّل ديناميكيًا وقت التشغيل لا عند الربط: winit عبر x11-dl وx11rb(dl-libxcb) وxkbcommon-dl وwayland-backend/dlopen، وwgpu يحمّل Vulkan بـ `ash::Entry::load()` وEGL عبر khronos-egl(dynamic)، وrfd يحمّل libdbus. وملف قفل Ruffle خالٍ من حزمة `xkbcommon` المربوطة. فالمتوقع أن يربط الملف التنفيذي صلبًا glibc وحدها، ومعها `libasound.so.2` إن استُعمل cpal/ALSA للصوت، وهي مثبّتة افتراضيًا على سطح مكتب Ubuntu وMint. ولهذا السبب نفسه يبني Ruffle نسخه بميزة `fontconfig-dlopen`.
  - evidence: https://github.com/rust-windowing/winit/blob/v0.30.13/Cargo.toml#L60-L71 و#L265-L290؛ https://github.com/gfx-rs/wgpu/blob/v30.0.1/wgpu-hal/src/vulkan/instance.rs#L576-L588؛ https://github.com/gfx-rs/wgpu/blob/v30.0.1/wgpu-hal/Cargo.toml#L243؛ C:\dev\ref\ruffle\Cargo.lock (x11rb يعتمد libloading، وwayland-sys يعتمد dlib، ولا xkbcommon، وalsa-sys لـ cpal 0.16.0)؛ C:\dev\ref\ruffle\.github\workflows\release.yml:126-128. يُتحقق نهائيًا بـ `readelf -d` على لينكس.
- **F14** [confirmed] اعتماديات البناء على Ubuntu 24.04: يبني Ruffle على مشغّلات ubuntu-24.04 بهذه الحزم: libasound2-dev وlibudev-dev وlibfontconfig-dev وlibfreetype6-dev وlibssl-dev وmesa-vulkan-drivers. يكفي مشروعَنا منها build-essential وpkg-config وlibasound2-dev، ومعها Java (default-jre-headless) لأن بناء ruffle_core يشغّل `java` لترجمة playerglobal الخاص بـ AS3 (وهذا عند البناء فقط لا عند التشغيل). لا حاجة إلى libssl-dev إلا مع ميزة native-tls الافتراضية في ruffle_frontend_utils، ولا إلى libudev-dev إلا مع gilrs، ولا إلى fontconfig أو freetype إلا مع ميزات Ruffle المكتبي، ولا إلى libgtk-3-dev إلا مع ميزة gtk3 في rfd. وقائمة README في eframe (libxcb-*-dev وlibxkbcommon-dev) قائمة محافظة قديمة الأثر ما دام التحميل ديناميكيًا.
  - evidence: C:\dev\ref\ruffle\.github\linux-dependencies.txt؛ C:\dev\ref\ruffle\README.md:58-68؛ C:\dev\ref\ruffle\tools\asc\src\lib.rs:49 (Command::new("java"))؛ C:\dev\ref\ruffle\core\build.rs:3-15؛ C:\dev\ref\ruffle\frontend-utils\Cargo.toml:13-28 و43-49؛ https://github.com/emilk/egui/blob/0.36.2/crates/eframe/README.md (السطر 24)
- **F15** [confirmed] الإعدادات النقّالة: يقرأ eframe الحقل `NativeOptions::persistence_path` على أنه مسار ملف RON (عبر `FileStorage::from_ron_filepath`) مع أن توثيقه يصفه بالمجلد. ودونه يُكتب في `%APPDATA%\<id>\data\app.ron` على ويندوز و`$XDG_DATA_HOME/<id>` على لينكس، فيلزم ضبطه صراحةً. وفي `dirs 6.0` تعيد `config_dir()` القيمة `$XDG_CONFIG_HOME` أو `~/.config` على لينكس، و`{FOLDERID_RoamingAppData}` على ويندوز. أما Ruffle نفسه فليس نقّالًا: يستعمل `dirs::config_local_dir()` ولا يغيّره إلا الخيار `--config`. وفي AppImage يضبط وقت التشغيل المتغيرات APPIMAGE (مسار الملف مطلقًا) وAPPDIR (نقطة التركيب، للقراءة فقط) وOWD وARGV0. ووضع AppImage النقّال يقوم على مجلدين بجوار الملف: `<name>.AppImage.home` يضبط HOME، و`<name>.AppImage.config` يضبط XDG_CONFIG_HOME، وdirs يحترم هذا الأخير تلقائيًا.
  - evidence: https://github.com/emilk/egui/blob/0.36.2/crates/eframe/src/epi.rs#L381-L383؛ crates/eframe/src/native/wgpu_integration.rs#L453-L463؛ crates/eframe/src/native/epi_integration.rs#L139-L146؛ crates/eframe/src/native/file_storage.rs#L17-L40 و#L117-L128؛ https://docs.rs/dirs/6.0.0/dirs/fn.config_dir.html؛ C:\dev\ref\ruffle\desktop\src\cli.rs:18-35 و140-142؛ https://docs.appimage.org/packaging-guide/environment-variables.html؛ https://docs.appimage.org/user-guide/portable-mode.html
- **F16** [confirmed] تغليف لينكس: وقت تشغيل AppImage التقليدي يحتاج libfuse2، واسم حزمته في Ubuntu 24.04 هو libfuse2t64 وليست مثبّتة افتراضيًا. أما وقت التشغيل الجديد type2-runtime فمربوط ساكنًا ولا يحتاجها، وتبقى `--appimage-extract-and-run` أو `APPIMAGE_EXTRACT_AND_RUN=1` ملاذًا أخيرًا. ويوزّع Ruffle على لينكس أرشيف tar.gz مسطّحًا فيه الملف التنفيذي، ومعه في مجلد extras ملف .desktop وmetainfo وأيقونة SVG؛ وهذا أبسط أشكال «بلا تثبيت».
  - evidence: https://docs.appimage.org/user-guide/troubleshooting/fuse.html؛ https://github.com/AppImage/type2-runtime (README: libfuse2 لم تعد مطلوبة لأن وقت التشغيل مربوط ساكنًا)؛ C:\dev\ref\ruffle\.github\workflows\release.yml:233-246
- **F17** [partly-wrong] ويندوز بملف تنفيذي واحد دون VC++ Redistributable: هذا مُثبت عمليًا، إذ يبني Ruffle نسخ ويندوز بـ `RUSTFLAGS=-Ctarget-feature=+crt-static` ويوزّع `ruffle.exe` وحده في zip، وفيه wgpu/DX12 وegui-wgpu وrfd وcpal. ومواصفة rustc للهدف windows-msvc فيها `crt_static_allows_dylibs: true` و`crt_static_respected: true`، فلا مشكلة مع proc-macros، وcc-rs يمرّر `/MT` متى رأى crt-static. والمحاذير: (1) متغير البيئة RUSTFLAGS يُلغي rustflags الموجودة في .cargo/config.toml كلها. (2) أي مكتبة ساكنة جاهزة مبنية بـ /MD تتعارض مع /MT، فتُجتنب ميزة `static-dxc` في wgpu. (3) d3d12.dll وdxgi.dll وd3dcompiler_47.dll مكوّنات من النظام تُحمَّل وقت التشغيل. (4) القيمة الافتراضية `Dx12Compiler::Auto` قد تلتقط dxcompiler.dll من PATH، فالأوثق تثبيت `Dx12Compiler::Fxc`، وهو لا يحتاج DLL إضافية.
  - evidence: C:\dev\ref\ruffle\.github\workflows\release.yml:152-164 و205-231؛ https://github.com/rust-lang/rust/blob/1.99.1/compiler/rustc_target/src/spec/base/windows_msvc.rs#L17-L18؛ https://github.com/rust-lang/cc-rs/blob/main/src/lib.rs (توثيق CARGO_CFG_TARGET_FEATURE: crt-static ⇒ -MT، السطر 205 تقريبًا)؛ https://doc.rust-lang.org/cargo/reference/config.html#buildrustflags؛ https://github.com/gfx-rs/wgpu/blob/v30.0.1/wgpu-types/src/backend.rs#L777-L804 و#L537-L555؛ `rustc --print target-features` المحلي يُظهر crt-static
  - CORRECTION: crt-static صحيح: الخيار RUSTFLAGS: -Ctarget-feature=+crt-static لهدفي windows-msvc في release.yml:152-164، والقيمتان crt_static_allows_dylibs: true وcrt_static_respected: true في windows_msvc.rs:17-18 على وسم 1.99.1، والقيمة الافتراضية Dx12Compiler::Auto ومعناها DXC الساكن ثم dxcompiler.dll من PATH ثم FXC (wgpu-types backend.rs:777-804). الخطأ: zip ويندوز لا يحوي ruffle.exe وحده، بل يحوي README.md وLICENSE.md وsetup.msi أيضًا، لأن خطوة MSI تكتب في package/ قبل تشغيل 7z (release.yml:211-231). وملاحظة: Ruffle نفسه يستعمل BackendOptions::default()، أي Auto لا Fxc (C:\dev\ref\ruffle\desktop\src\gui\controller.rs:604-607)، فتثبيت Fxc يخالف ما اختبره Ruffle.
- **F18** [confirmed] تفاصيل ويندوز: (أ) يضيف Ruffle المكتبي `/STACK:4000000` للربط مع MSVC لأن ملفات SWF ذات التعاود العميق تُفيض المكدّس الافتراضي (1MB)، وسيعمل المشغّل عندنا على الخيط الرئيس لـ eframe فيلزمنا الأمر نفسه. (ب) `#![windows_subsystem = "windows"]` يمنع ظهور نافذة الطرفية. (ج) أيقونة الملف التنفيذي اختيارية عبر `winresource 0.1.34` في build.rs، وتحتاج rc.exe من Windows SDK عند البناء فقط. (د) لا حاجة إلى ملف manifest: winit يفعّل وعي DPI وقت التشغيل (PER_MONITOR_AWARE_V2)، وحوار ملفات rfd قائم على COM (IFileDialog) دون حاجة إلى common-controls v6.
  - evidence: C:\dev\ref\ruffle\desktop\build.rs:24-30 و38-51 و64-92؛ C:\dev\ref\ruffle\desktop\src\main.rs:7؛ C:\dev\ref\ruffle\desktop\Cargo.toml (winresource = "0.1"، وفي القفل 0.1.34)؛ https://github.com/rust-windowing/winit/blob/v0.30.13/src/platform_impl/windows/dpi.rs#L20-L31 وevent_loop.rs#L199؛ https://github.com/PolyMeilex/rfd/blob/0.17.2/Cargo.toml#L20
- **F19** [confirmed] النص العربي في واجهة egui 0.36 (الغلاف لا المحتوى): epaint يشكّل الحروف بـ harfrust ويستدعي guess_segment_properties، فالكلمات العربية الخالصة تتصل حروفها. لكنه لا يعيد ترتيب الاتجاه الثنائي، والتعليقات نفسها تقول ذلك (TODO heed bidi characters، وأن RTL/bidi لم يُضف بعد)، فالسطور المختلطة (عربي مع لاتيني أو أرقام) قد تنعكس. وخطوط egui الافتراضية بلا خط عربي، فيلزم تضمين خط (Noto Naskh أو Noto Sans Arabic برخصة OFL) عبر `FontDefinitions::font_data: BTreeMap<String, Arc<FontData>>` و`FontData::from_static`. وحيلة `reorder_bidi` في Ruffle (عكس الحروف يدويًا) سابقة على harfrust، فلا تُنقل كما هي لأنها قد تعكس النص مرتين.
  - evidence: https://github.com/emilk/egui/blob/0.36.2/crates/epaint/src/text/font.rs#L829؛ crates/epaint/src/text/text_layout.rs#L1363-L1366 و#L1400-L1434؛ crates/epaint/src/text/fonts.rs#L125 و#L431-L443؛ C:\dev\ref\ruffle\desktop\src\gui\locale.rs:54-86
- **F20** [confirmed] إعادة المحاولة وإعادة التشغيل داخل العملية: القيمة الافتراضية `run_and_return: true`، ويحتفظ eframe بحلقة الأحداث في متغير thread_local ويعيد استعمالها؛ فيمكن استدعاء `run_native` مرة ثانية بعد Err. يتيح هذا تراجعًا آليًا من GPU إلى CPU، وتبديل العارض بإعادة تشغيل داخلية دون إنهاء العملية. ويحفظ مسار الخطأ النتيجة في return_result ويخرج من الحلقة. وتنبيه: `WgpuConfiguration::default()` يضع HIGH_THROUGHPUT، بينما NativeOptions الافتراضي يضع LOW_LATENCY، فيُحافظ عليه بـ `.with_surface_config(SurfaceConfig::LOW_LATENCY)` عند تخصيص wgpu_options.
  - evidence: https://github.com/emilk/egui/blob/0.36.2/crates/eframe/src/native/run.rs#L53-L76 و#L161-L165 و#L385-L395 و#L449-L471؛ crates/eframe/src/epi.rs#L330-L342 و#L459-L461؛ crates/egui-wgpu/src/lib.rs#L375-L386
- **F21** [partly-wrong] توفّر العارض البرمجي على لينكس: lavapipe جزء من حزمة mesa-vulkan-drivers، وRuffle يثبّتها في CI ليختبر الرسم على مشغّلات بلا GPU. والبديل الثاني llvmpipe عبر GL/EGL، وتوفّره libegl-mesa0 وlibgl1-mesa-dri المثبّتتان مع Mesa. أما أن mesa-vulkan-drivers مثبّتة افتراضيًا على Mint 22 Cinnamon فلم أتحقق منه. على ويندوز 10 و11، WARP جزء من النظام.
  - evidence: C:\dev\ref\ruffle\.github\linux-dependencies.txt:6؛ C:\dev\ref\ruffle\.github\workflows\test_rust.yml:84 و145 و228؛ https://github.com/gfx-rs/wgpu/blob/v30.0.1/wgpu-hal/src/gles/adapter.rs#L144-L155
  - CORRECTION: الصحيح منه: linux-dependencies.txt:6 هو mesa-vulkan-drivers، وtest_rust.yml:84 و145 و228 تثبّت الملف كله. لكن هذا لا يثبت أن Ruffle تثبّتها «لتختبر الرسم على مشغّلات بلا GPU»، فهذا استنتاج لا يقوله الملف. والقول بأن llvmpipe عبر GL متاح من libegl-mesa0 وlibgl1-mesa-dri لم يُستشهد عليه. وتوفّر الحزم افتراضيًا على Mint 22 غير متحقَّق منه، كما أقرّ التقرير.
- **F22** [unverifiable] لا يصلح ربط ساكن كامل بـ musl على لينكس، لأن winit وwgpu وrfd تعتمد على dlopen، والملفات الساكنة بـ musl لا تستطيعه؛ فالباقي glibc مع التحميل الديناميكي. والبناء على Ubuntu 24.04 يجعل الحد الأدنى glibc 2.39، وهذا يناسب Ubuntu 24.04 وMint 22 (المبنية على noble). ولتوسيع النطاق يُبنى على توزيعة أقدم.
  - evidence: استنتاج من F13 (كل الواجهات بـ dlopen)؛ C:\dev\ref\ruffle\.github\workflows\release.yml:126-128 (البناء على ubuntu-24.04 بـ glibc ديناميكي)
  - CORRECTION: استنتاج معقول ولا دليل مباشر عليه في المصدر. صحيح أن musl الساكن لا يدعم dlopen لمكتبات glibc. وأن glibc 2.39 هي نسخة Ubuntu 24.04 معلومة عامة لم يُستشهد عليها. ولم يُختبر شيء من ذلك.

## Risks

- النص العربي في واجهة egui 0.36: الحروف تتصل بفضل harfrust، لكن لا ترتيب ثنائي الاتجاه؛ فالعناوين المختلطة (عربي مع أرقام أو لاتيني) قد تظهر مقلوبة الأجزاء. يلزم اختبار مبكر، وقد يلزم ترتيب مسبق بـ unicode-bidi لكل سطر مختلط، مع الحذر من نقل حيلة Ruffle في عكس الحروف لأنها سابقة على التشكيل.
- Wayland: winit 0.30.13 لا ينفّذ إفلات الملفات، فيلزم إعداد «فرض X11» أو الاكتفاء بحوار الفتح. وموضع النافذة وcentered غير مدعومين هناك.
- أداء وضع CPU: يرسم WARP وlavapipe على المعالج؛ معاينة بحجم مسرح كبير بـ 30-60 إطارًا قد تثقل. يُخفَّف ذلك بخفض دقة نسيج المعاينة وبالاعتماد على time_til_next_frame بدل الإعادة المستمرة.
- إغفال حذف VALIDATION_INDIRECT_CALL من instance flags (وهو حاضر افتراضيًا حتى في release) قد يُفشل إنشاء الجهاز على بعض البطاقات (wgpu#8799)، كما نبّه Ruffle.
- إعادة run_native بعد الخطأ (التراجع من GPU إلى CPU أو التبديل) تعتمد على إعادة eframe استعمال حلقة الأحداث thread_local؛ مقروءة من المصدر لكنها لم تُجرَّب عمليًا. وقد تومض نافذة ثم تُغلق قبل المحاولة الثانية.
- persistence_path في eframe مسار ملف RON رغم أن توثيقه يقول مجلد؛ وإعطاؤه مجلدًا يُفشل الحفظ بصمت.
- التسجيل في كل إطار بـ register_native_texture يسرّب معرّفات ومجموعات ربط؛ وإهمال update_egui_texture_from_wgpu_texture بعد resize يُبقي المعاينة على النسيج القديم.
- crt-static: متغير RUSTFLAGS في البيئة يُلغي rustflags الموجودة في config بصمت، فيعود الاعتماد على VCRUNTIME140.dll. وأي مكتبة جاهزة مبنية بـ /MD (مثل static-dxc) تكسر الربط. يُتحقق بـ dumpbin /dependents.
- Dx12Compiler::Auto قد يلتقط dxcompiler.dll غريبة من PATH على أجهزة المستخدمين، فيُثبَّت Fxc.
- AppImage بوقت التشغيل القديم يتطلب libfuse2t64 غير المثبّتة افتراضيًا على Ubuntu 24.04؛ يلزم type2-runtime الساكن، أو توزيع tar.gz.
- ملف exe غير موقَّع ينزل من الشبكة يستدعي تحذير SmartScreen وعلامة Mark-of-the-Web؛ وهذا يمسّ «سهولة التشغيل» لا التثبيت.
- ruffle_frontend_utils بميزاتها الافتراضية (native-tls) يربط libssl.so.3 على لينكس، ويُدخل reqwest على كل حال، فيُعطَّل الافتراضي أو تُكتب واجهة صوت cpal خاصة.
- بناء ruffle_core يحتاج Java عند البناء (tools/asc يستدعي java) على ويندوز ولينكس وفي CI، مع أن الهدف AS2 فقط.
- البناء على Ubuntu 24.04 يضع الحد الأدنى glibc 2.39؛ فالتوزيعات الأقدم لن تشغّل الملف.
- على هذا الجهاز تحديدًا: يجب ألا يُختبر وضع GPU أبدًا (UPS ضعيف). JAZARI_FORCE_CPU في config المحلي لـ cargo والمحدِّد الذي يرفض غير Cpu يمنعان ذلك، فلا يُضاف أي تراجع آلي من CPU إلى GPU.

## Open questions

- هل يكون الوضع النقّال افتراضيًا على ويندوز ولينكس (الكتابة بجوار البرنامج متى أمكنت)، أم بملف علامة jazari.portable فقط؟ التوصية: افتراضي لـ exe وtar.gz، واختياري لـ AppImage، وهذا رأي في تصميم المنتج لا حقيقة تقنية.
- صيغة التوزيع على لينكس: أرشيف tar.gz مسطّح كما يفعل Ruffle (أبسط ولا يحتاج FUSE)، أم AppImage بـ type2-runtime الساكن (تكامل أفضل مع سطح المكتب)، أم كلاهما؟
- هل mesa-vulkan-drivers (lavapipe) وzenity وxdg-desktop-portal-xapp مثبّتة افتراضيًا على Linux Mint 22 Cinnamon؟ يُتحقق على جهاز فعلي (ls /usr/share/vulkan/icd.d/lvp_icd*.json).
- ما مستوى الأداء المقبول للمعاينة على WARP وlavapipe؟ يُقاس بملف SWF نموذجي بأبعاد 550x400 و1280x720 قبل تحديد الدقة الافتراضية للنسيج.
- هل تُعرض واجهة البرنامج بالعربية من الإصدار الأول؟ إن كان الجواب نعم فيلزم اختبار harfrust مع عناوين مختلطة، وتعريب نصوص egui_dock عبر Translations.
- هل تُضاف ميزة glow إلى eframe ملاذًا أخيرًا للواجهة بلا معاينة إذا لم يوجد أي محوّل wgpu؟ تكلفتها حجم أكبر وتعقيد، ونفعها نادر لأن WARP موجود دائمًا على ويندوز 10/11.
- هل يُوقَّع ملف exe رقميًا لتفادي SmartScreen في التوزيع العام؟

## Verifier: problems in the code sketch

- ثغرة في حماية هذا الجهاز من GPU: لا يُقرأ JAZARI_FORCE_CPU إلا مرة واحدة قبل الحلقة. وزر «التحويل إلى Gpu» في القائمة يضع restart = Some(Gpu)، فتعيد الحلقة التشغيل في وضع GPU وتستدعي settings.save، والمحدِّد في وضع Gpu يرفض محوّلات Cpu. فنقرة واحدة تُشغّل العتاد الرسومي وتحفظ ذلك في الإعدادات. الإصلاح: عند كل دورة، إن وُجد المتغير يُفرض mode = Cpu، ويُعطَّل خيار GPU في القائمة.
- الملفان app.rs وpreview.rs ينقصهما الاستيراد: Arc وRc وCell وegui وeframe وegui_wgpu وwgpu وcrate::{paths, preview, RenderMode}. فهما لا يترجمان وحدتين مستقلتين.
- ترتيب تغيير الحجم: تُستدعى sync_texture وtick_and_render قبل DockArea، بينما Preview::show (وفيها set_viewport_dimensions ثم RenderTarget::resize) تأتي داخل DockArea بعدهما؛ فيُعرض النسيج القديم إطارًا واحدًا بعد كل تغيير حجم. الأصح نقل التسجيل والرسم إلى آخر ui() بعد show_inside، لأن eframe لا يرسم egui إلا بعد عودة ui()، فيبقى الترتيب على الطابور مضمونًا.
- واصف الجهاز يطلب adapter.limits() كاملة، ولا يضيف GpuProfiler::ALL_WGPU_TIMER_FEATURES. وهذا يخالف تهيئة Ruffle المختبَرة: downlevel_webgl2_defaults مع using_resolution وusing_alignment وحقلين مرفوعين وmax_color_attachments = 4 (C:\dev\ref\ruffle\render\wgpu\src\backend.rs:1218-1239). هو صالح غالبًا، لكنه تهيئة لم يختبرها Ruffle. الأسلم نسخ حدود Ruffle، مع ضمان max_texture_dimension_2d >= 8192 كما يطلب egui (setup.rs:269-274).
- Dx12Compiler::Fxc يخالف Ruffle الذي يمرّر BackendOptions::default() فيكون Auto (controller.rs:604-607). وزمن ترجمة مظلّلات Ruffle بـ FXC على WARP لم يُقَس.
- في بناء debug تعيد from_build_config() القيمة DEBUG|VALIDATION|VALIDATION_INDIRECT_CALL (wgpu-types instance.rs:253-255 و268-274). فحذف INDIRECT وحده يُبقي VALIDATION مفعّلًا، وهذا بطيء جدًا على WARP وlavapipe، ويطلب طبقة تصحيح D3D12 (Graphics Tools). الأفضل في وضع CPU ضبط الرايات صراحةً.
- event_loop_builder (فرض X11) لا يُطبَّق إلا عند إنشاء الحلقة أول مرة (eframe run.rs:45-47 و53-76)؛ فتغيير force_x11 لا يسري بإعادة run_native داخل الحلقة، بل يحتاج إعادة تشغيل العملية.
- with_icon(from_png_bytes(..).unwrap_or_default()) يمرّر IconData فارغة إن فشل فك الترميز؛ والأنظف أن تُستعمل with_icon فقط عند النجاح.
- الاعتماد المباشر winit = { default-features = false } يستند إلى توحيد الميزات ليفعّل x11 من أجل EventLoopBuilderExtX11. والأوثق استعمال eframe::egui_winit::winit (egui-winit lib.rs:21 فيه `pub use winit;`)، أو إضافة features = ["x11"] صراحةً.
- رسالة الخطأ النهائية عبر rfd::MessageDialog تمر على لينكس بـ zenity (xdg_desktop_portal.rs:291-303)؛ فإن لم يكن zenity مثبّتًا فشل العرض بصمت. يلزم ملاذ: كتابة الخطأ في ملف سجل داخل مجلد البيانات وعلى stderr.

## Verifier: missed points

- Ruffle المكتبي لا يستعمل eframe أصلًا، بل يدير حلقة winit بنفسه مع egui-winit وegui-wgpu (C:\dev\ref\ruffle\desktop\Cargo.toml:19، وcontroller.rs:55-113 و445-494). فالجمع بين eframe وruffle_core على جهاز واحد تركيب لم يُختبر في أي مكان. والبديل الموثوق أن نحاكي معمار Ruffle نفسه، فنتحكم في إنشاء الجهاز وترتيب الرسم دون قيود eframe.
- لوح المسرح يرسمه egui، وسيعرض نصوص المستند العربية أثناء التحرير. وegui لا يرتّب الاتجاه الثنائي (font.rs:829، وtext_layout.rs:1366)، فلا يكفي لعرض نص المحتوى من اليمين إلى اليسار. يلزم مسار تشكيل وترتيب خاص بالمسرح (harfrust مع unicode-bidi)، أو رسم المسرح عبر Ruffle أو عبر مُفسِّس خاص. ناقش التقرير نصوص الواجهة وحدها.
- حتى في وضع CPU، يُنشئ تعداد محوّلات DX12 جهاز D3D12 على كل محوّل DXGI، ومنها البطاقة الحقيقية (wgpu-hal dx12/instance.rs:166-188، وdx12/adapter.rs:92-93)، وVulkan يفعل مثل ذلك. هذه تهيئة خفيفة للمعرِّف لا عبء عمل، لكن ينبغي أن يعرفها المستخدم بسبب قيد الـ UPS.
- تعارض COM على ويندوز: يُفعَّل سحب الملفات وإفلاتها في winit افتراضيًا عبر OLE، ويتعارض مع أي تهيئة COMINIT_MULTITHREADED على الخيط نفسه (winit src/platform/windows.rs:491-497). فإن أُضيف صوت المعاينة (cpal/WASAPI) لزم أن يعمل على خيط مستقل، أو يُتحقق من نوع الشقة.
- يستدعي eframe الدالة logic() بدل ui() ما دامت النافذة مخفية (epi.rs:153-167)؛ فمعاينة Ruffle المربوطة بـ ui() تتوقف عند التصغير. يُقرَّر هل يُنقل tick إلى logic().
- Ruffle يستعمل حوارات غير متزامنة (AsyncFileDialog في C:\dev\ref\ruffle\desktop\src\backends\ui.rs:11 و200-202). أما rfd::FileDialog::pick_file داخل ui() فيوقف حلقة eframe، ويمر على لينكس بـ pollster::block_on، فتتجمد المعاينة طوال فتح الحوار.
- متطلبات بناء ruffle_core على ويندوز: يلزم Java وقت البناء (core/build.rs:3-10)، وقد يلزم أيضًا تثبيت ميزات ruffle_core الافتراضية بعناية. وفي التقرير ورد ذلك للينكس وحده دون خطة CI لويندوز (windows-2025 كما يفعل Ruffle في release.yml:157-164).
- التحقق النهائي من النقّالية يحتاج اختبارًا فعليًا لم يُذكر: dumpbin /dependents على exe المبني بـ crt-static، وreadelf -d على لينكس مع إلغاء ربط wayland-dlopen. يُجرى ذلك في CI بلا GPU مع WARP وlavapipe، ولا يُجرى على هذا الجهاز في وضع GPU.

## Code sketch (researcher; check against corrections above)
```rust
// ================= Cargo.toml (مقتطف) =================
// [dependencies]
// eframe     = { version = "0.36.2", features = ["persistence"] }  # الافتراضي يبقى: wgpu, wayland, x11, winit/default, accesskit, default_fonts, links
// egui_dock  = { version = "0.21.1", features = ["serde"] }
// wgpu       = "30.0.1"                                            # يجب أن يتوحّد مع wgpu الذي يستعمله ruffle_render_wgpu
// rfd        = { version = "0.17.2", default-features = false, features = ["xdg-portal", "wayland"] }
// dirs       = "6.0"
// serde      = { version = "1.0", features = ["derive"] }
// toml       = "1.1"
// log        = "0.4"
// ruffle_core        = { git = "https://github.com/ruffle-rs/ruffle", rev = "07d3e430631333f80f754df01a968bdcd80d98c5" }
// ruffle_render_wgpu = { git = "https://github.com/ruffle-rs/ruffle", rev = "07d3e430631333f80f754df01a968bdcd80d98c5" }
// [target.'cfg(target_os = "linux")'.dependencies]
// winit = { version = "0.30.13", default-features = false }        # لـ EventLoopBuilderExtX11 فقط
// [build-dependencies]
// winresource = "0.1.34"                                            # اختياري (أيقونة exe)
// [profile.release] panic = "abort"
// [profile.dist]    inherits = "release", lto = true, codegen-units = 1
//
// ================= .cargo/config.toml (في المستودع) =================
// [target.x86_64-pc-windows-msvc]
// rustflags = ["-C", "target-feature=+crt-static"]
// ================= C:\dev\rust\cargo\config.toml (محلي لهذا الجهاز فقط) =================
// [env]
// JAZARI_FORCE_CPU = "1"      # يحمي هذا الجهاز: cargo run لا يلمس GPU أبدًا
//
// ================= build.rs =================
fn main() {
    let target = std::env::var("TARGET").unwrap();
    if target.contains("windows-msvc") {
        println!("cargo:rustc-link-arg-bins=/STACK:4000000"); // كما في ruffle/desktop/build.rs:38-51
    }
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon.ico");
        let _ = res.compile();
    }
}

// ================= src/main.rs =================
#![windows_subsystem = "windows"]
use std::{cell::Cell, rc::Rc, sync::Arc};
use eframe::{egui, egui_wgpu, wgpu};
mod app; mod paths; mod preview;

#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
pub enum RenderMode { Gpu, Cpu }

pub const APP_ID: &str = "org.aljazari.Jazari"; // = اسم ملف .desktop = معرّف Wayland

fn main() -> eframe::Result {
    let paths = paths::AppPaths::resolve();
    let mut settings = paths::Settings::load(&paths.settings_file());
    if std::env::var_os("JAZARI_FORCE_CPU").is_some() { settings.render_mode = RenderMode::Cpu; }
    let mut mode = settings.render_mode;

    loop {
        let restart: Rc<Cell<Option<RenderMode>>> = Rc::new(Cell::new(None));
        let options = eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_title("الجزري")
                .with_app_id(APP_ID)
                .with_inner_size([1280.0, 800.0])
                .with_drag_and_drop(true)
                .with_icon(eframe::icon_data::from_png_bytes(include_bytes!("../assets/icon.png")).unwrap_or_default()),
            renderer: eframe::Renderer::Wgpu,
            wgpu_options: wgpu_config(mode),
            persistence_path: Some(paths.dir.join("ui-state.ron")), // مسار «ملف» RON وليس مجلدًا
            run_and_return: true, // يسمح باستدعاء run_native مجددًا (حلقة أحداث thread_local)
            #[cfg(target_os = "linux")]
            event_loop_builder: settings.force_x11.then(|| -> eframe::EventLoopBuilderHook {
                Box::new(|b: &mut eframe::EventLoopBuilder<eframe::UserEvent>| {
                    use winit::platform::x11::EventLoopBuilderExtX11 as _;
                    b.with_x11(); // XWayland: يعيد السحب والإفلات على جلسات Wayland
                })
            }),
            ..Default::default()
        };
        let (s, p, r) = (settings.clone(), paths.clone(), restart.clone());
        let result = eframe::run_native(APP_ID, options, Box::new(move |cc| {
            Ok(Box::new(app::JazariApp::new(cc, s, p, r)?) as Box<dyn eframe::App>)
        }));
        match result {
            Ok(()) => match restart.take() {
                Some(m) => { mode = m; settings.render_mode = m; settings.save(&paths.settings_file()); }
                None => return Ok(()),
            },
            // تراجع باتجاه واحد فقط: GPU ← CPU، ولا يُنتقل آليًا من CPU إلى GPU أبدًا
            Err(eframe::Error::Wgpu(e)) if mode == RenderMode::Gpu => {
                log::warn!("GPU init failed: {e}; retrying with software renderer");
                mode = RenderMode::Cpu;
            }
            Err(e) => {
                rfd::MessageDialog::new()
                    .set_level(rfd::MessageLevel::Error)
                    .set_title("الجزري")
                    .set_description(format!("تعذّرت تهيئة العارض:\n{e}\n{}", software_hint()))
                    .show();
                return Err(e);
            }
        }
    }
}

fn wgpu_config(mode: RenderMode) -> egui_wgpu::WgpuConfiguration {
    let backends = match (cfg!(windows), mode) {
        (true, RenderMode::Cpu) => wgpu::Backends::DX12,                       // WARP لا يأتي إلا عبر DX12
        (true, RenderMode::Gpu) => wgpu::Backends::DX12 | wgpu::Backends::VULKAN,
        (false, _) => wgpu::Backends::VULKAN | wgpu::Backends::GL,             // lavapipe ثم llvmpipe(GL)
    };
    let selector: egui_wgpu::NativeAdapterSelectorMethod = Arc::new(
        move |adapters: &[wgpu::Adapter], surface: Option<&wgpu::Surface<'_>>| pick_adapter(mode, adapters, surface),
    );
    let setup = egui_wgpu::WgpuSetupCreateNew {
        instance_descriptor: wgpu::InstanceDescriptor {
            backends: backends.with_env(),
            // مطابق لـ ruffle_render_wgpu::backend::create_wgpu_instance (wgpu#8799)
            flags: wgpu::InstanceFlags::from_build_config()
                .difference(wgpu::InstanceFlags::VALIDATION_INDIRECT_CALL)
                .with_env(),
            memory_budget_thresholds: Default::default(),
            backend_options: wgpu::BackendOptions {
                dx12: wgpu::Dx12BackendOptions { shader_compiler: wgpu::Dx12Compiler::Fxc, ..Default::default() },
                ..Default::default()
            },
            display: None, // يحقن eframe مقبض العرض (OwnedDisplayHandle) بنفسه
        },
        native_adapter_selector: Some(selector),
        // جهاز واحد يكفي egui وRuffle معًا: حدود المحوّل كاملةً + ميزات Ruffle الاختيارية
        device_descriptor: Arc::new(|adapter: &wgpu::Adapter| wgpu::DeviceDescriptor {
            label: Some("jazari: egui + ruffle"),
            required_features: adapter.features()
                & (wgpu::Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES
                    | wgpu::Features::TEXTURE_COMPRESSION_BC
                    | wgpu::Features::FLOAT32_FILTERABLE),
            required_limits: adapter.limits(),
            memory_hints: Default::default(),
            trace: wgpu::Trace::Off,
            experimental_features: wgpu::ExperimentalFeatures::disabled(),
        }),
        ..egui_wgpu::WgpuSetupCreateNew::without_display_handle()
    };
    egui_wgpu::WgpuConfiguration { wgpu_setup: setup.into(), ..Default::default() }
        .with_surface_config(egui_wgpu::SurfaceConfig::LOW_LATENCY)
}

fn pick_adapter(mode: RenderMode, adapters: &[wgpu::Adapter], surface: Option<&wgpu::Surface<'_>>)
    -> Result<wgpu::Adapter, String>
{
    use wgpu::{Backend, DeviceType};
    let mut c: Vec<&wgpu::Adapter> = adapters.iter()
        .filter(|a| surface.is_none_or(|s| a.is_surface_supported(s)))
        .filter(|a| {
            let t = a.get_info().device_type;
            match mode { RenderMode::Cpu => t == DeviceType::Cpu, RenderMode::Gpu => t != DeviceType::Cpu }
        })
        .collect();
    c.sort_by_key(|a| {
        let i = a.get_info();
        let backend = match i.backend { Backend::Dx12 => 0, Backend::Vulkan => 1, _ => 2 };
        let kind = match i.device_type {
            DeviceType::DiscreteGpu => 0, DeviceType::IntegratedGpu => 1, DeviceType::VirtualGpu => 2,
            DeviceType::Other => 3, DeviceType::Cpu => 4,
        };
        (backend, kind)
    });
    c.first().map(|a| (*a).clone()).ok_or_else(|| match mode {
        RenderMode::Cpu => software_hint().to_owned(),
        RenderMode::Gpu => "لم يُعثر على معالج رسوميات صالح".to_owned(),
    })
}

fn software_hint() -> &'static str {
    if cfg!(windows) { "لا يتوفر WARP (DX12 البرمجي)؛ يلزم Windows 10 أو أحدث." }
    else { "لا يتوفر عارض برمجي؛ ثبّت: sudo apt install mesa-vulkan-drivers libegl-mesa0" }
}

// ================= src/paths.rs =================
use std::path::{Path, PathBuf};
#[derive(Clone)]
pub struct AppPaths { pub dir: PathBuf, pub portable: bool }
impl AppPaths {
    pub fn settings_file(&self) -> PathBuf { self.dir.join("settings.toml") }
    pub fn resolve() -> Self {
        const DATA: &str = "JazariData";        // مجلد البيانات بجوار البرنامج
        const MARKER: &str = "jazari.portable"; // ملف فارغ يفرض الوضع النقّال
        if let Some(d) = std::env::var_os("JAZARI_HOME").map(PathBuf::from) {
            if writable(&d) { return Self { dir: d, portable: true }; }
        }
        // في AppImage يقع الملف التنفيذي داخل /tmp/.mount_* (للقراءة فقط) فنعتمد مجلد $APPIMAGE
        let appimage = std::env::var_os("APPIMAGE").map(PathBuf::from);
        let base = appimage.as_deref().and_then(Path::parent).map(Path::to_path_buf)
            .or_else(|| std::env::current_exe().ok()?.parent().map(Path::to_path_buf));
        if let Some(base) = base {
            let data = base.join(DATA);
            let opted_in = base.join(MARKER).exists() || data.is_dir();
            let default_portable = appimage.is_none(); // exe على ويندوز / أرشيف tar: نقّال افتراضيًا إن أمكنت الكتابة
            if (opted_in || default_portable) && writable(&data) { return Self { dir: data, portable: true }; }
        }
        // ملاذ: مجلد المنصّة (و"<name>.AppImage.config" يضبط XDG_CONFIG_HOME فيحترمه dirs)
        let dir = dirs::config_dir().unwrap_or_else(std::env::temp_dir).join("Al-Jazari");
        let _ = std::fs::create_dir_all(&dir);
        Self { dir, portable: false }
    }
}
fn writable(dir: &Path) -> bool {
    if std::fs::create_dir_all(dir).is_err() { return false; }
    let probe = dir.join(".write-probe");
    let ok = std::fs::write(&probe, b"").is_ok();
    let _ = std::fs::remove_file(&probe);
    ok
}

// ================= src/app.rs =================
use egui_dock::{DockArea, DockState, NodeIndex, Style, TabViewer};
use ruffle_render_wgpu::descriptors::Descriptors;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, serde::Serialize, serde::Deserialize)]
pub enum Tab { Stage, Timeline, Library, Properties, Actions, Output, Preview }

pub struct JazariApp {
    dock: DockState<Tab>,
    ruffle_gpu: Arc<Descriptors>,          // تُنشأ مرة لكل جهاز وتُعاد لكل معاينة
    preview: Option<preview::Preview>,
    adapter_summary: String,
    settings: paths::Settings,
    paths: paths::AppPaths,
    restart: Rc<Cell<Option<RenderMode>>>,
    log: Vec<String>,
}

impl JazariApp {
    pub fn new(cc: &eframe::CreationContext<'_>, settings: paths::Settings, paths: paths::AppPaths,
               restart: Rc<Cell<Option<RenderMode>>>) -> Result<Self, String> {
        install_fonts(&cc.egui_ctx);
        let rs = cc.wgpu_render_state.as_ref().ok_or("wgpu render state missing")?;
        // جهاز eframe نفسه يُسلَّم إلى Ruffle: لا جهاز ثانٍ
        let ruffle_gpu = Arc::new(Descriptors::new(
            rs.instance.clone(), rs.adapter.clone(), rs.device.clone(), rs.queue.clone()));
        let dock = cc.storage.and_then(|s| eframe::get_value(s, "dock")).unwrap_or_else(default_layout);
        Ok(Self { dock, ruffle_gpu, preview: None,
                  adapter_summary: egui_wgpu::adapter_info_summary(&rs.adapter.get_info()),
                  settings, paths, restart, log: Vec::new() })
    }
    fn open(&mut self, _path: &std::path::Path) { /* تحميل XFL: موضوع آخر */ }
}

fn default_layout() -> DockState<Tab> {
    let mut dock = DockState::new(vec![Tab::Stage, Tab::Preview]);
    let s = dock.main_surface_mut();
    let [stage, _] = s.split_above(NodeIndex::root(), 0.70, vec![Tab::Timeline]);
    let [stage, _] = s.split_right(stage, 0.76, vec![Tab::Properties, Tab::Library]);
    let _ = s.split_below(stage, 0.72, vec![Tab::Actions, Tab::Output]);
    dock
}

fn install_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert("arabic".into(), Arc::new(egui::FontData::from_static(
        include_bytes!("../assets/fonts/NotoNaskhArabic-Regular.ttf"))));
    for fam in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        fonts.families.entry(fam).or_default().push("arabic".into()); // احتياطي بعد الخط اللاتيني
    }
    ctx.set_fonts(fonts);
}

impl eframe::App for JazariApp {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        // ملفات مُفلَتة (ويندوز/X11؛ لا تصل على Wayland الأصلي مع winit 0.30)
        let dropped: Vec<std::path::PathBuf> =
            ui.ctx().input(|i| i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).collect());
        for p in dropped { self.open(&p); }

        egui::Panel::top("menu").show(ui, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                ui.menu_button("ملف", |ui| {
                    if ui.button("فتح…").clicked() {
                        if let Some(p) = rfd::FileDialog::new().add_filter("XFL", &["xfl"]).set_parent(&*frame).pick_file() {
                            self.open(&p);
                        }
                    }
                });
                ui.menu_button("إعدادات", |ui| {
                    let other = if self.settings.render_mode == RenderMode::Gpu { RenderMode::Cpu } else { RenderMode::Gpu };
                    if ui.button(format!("التحويل إلى {other:?} (يعيد التشغيل)")).clicked() {
                        self.restart.set(Some(other));
                        ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                    ui.label(&self.adapter_summary);
                });
            });
        });

        // يرسم Ruffle قبل أن يرسم eframe واجهة egui: الطابور واحد فالترتيب مضمون
        if let (Some(p), Some(rs)) = (self.preview.as_mut(), frame.wgpu_render_state()) {
            p.sync_texture(rs);
            p.tick_and_render();
            ui.ctx().request_repaint_after(p.time_til_next_frame());
        }

        let mut viewer = Panels { preview: self.preview.as_mut(), log: &self.log };
        DockArea::new(&mut self.dock)
            .style(Style::from_egui(ui.style().as_ref()))
            .show_inside(ui, &mut viewer);
    }
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, "dock", &self.dock);
    }
}

struct Panels<'a> { preview: Option<&'a mut preview::Preview>, log: &'a [String] }
impl TabViewer for Panels<'_> {
    type Tab = Tab;
    fn id(&mut self, tab: &mut Tab) -> egui::Id { egui::Id::new(*tab) }
    fn title(&mut self, tab: &mut Tab) -> egui::WidgetText {
        match tab {
            Tab::Stage => "المسرح", Tab::Timeline => "الخط الزمني", Tab::Library => "المكتبة",
            Tab::Properties => "الخصائص", Tab::Actions => "الإجراءات", Tab::Output => "المخرجات",
            Tab::Preview => "المعاينة",
        }.into()
    }
    fn ui(&mut self, ui: &mut egui::Ui, tab: &mut Tab) {
        match tab {
            Tab::Preview => match self.preview.as_deref_mut() {
                Some(p) => p.show(ui),
                None => { ui.label("انشر المستند لمعاينته"); }
            },
            Tab::Output => for line in self.log { ui.label(line); },
            _ => { ui.label("…"); }
        }
    }
    fn is_closeable(&self, tab: &Tab) -> bool { !matches!(tab, Tab::Stage | Tab::Timeline) }
}

// ================= src/preview.rs =================
use std::sync::Mutex;
use ruffle_render_wgpu::target::{RenderTarget, RenderTargetFrame};
type Fresh = Arc<Mutex<Option<wgpu::TextureView>>>;

#[derive(Debug)]
pub struct PreviewTarget { texture: wgpu::Texture, fresh: Fresh }
#[derive(Debug)]
pub struct PreviewFrame(wgpu::TextureView);
impl RenderTargetFrame for PreviewFrame {
    fn into_view(self) -> wgpu::TextureView { self.0 }
    fn view(&self) -> &wgpu::TextureView { &self.0 }
}
impl PreviewTarget {
    fn create(device: &wgpu::Device, w: u32, h: u32, fresh: Fresh) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("ruffle preview"),
            size: wgpu::Extent3d { width: w.max(1), height: h.max(1), depth_or_array_layers: 1 },
            mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm, // شرط register_native_texture
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        *fresh.lock().unwrap() = Some(texture.create_view(&Default::default()));
        Self { texture, fresh }
    }
}
impl RenderTarget for PreviewTarget { // على نمط MovieView في ruffle/desktop/src/gui/movie.rs
    type Frame = PreviewFrame;
    fn resize(&mut self, device: &wgpu::Device, w: u32, h: u32) { *self = Self::create(device, w, h, self.fresh.clone()); }
    fn format(&self) -> wgpu::TextureFormat { self.texture.format() }
    fn width(&self) -> u32 { self.texture.width() }
    fn height(&self) -> u32 { self.texture.height() }
    fn get_next_texture(&mut self) -> Option<PreviewFrame> { Some(PreviewFrame(self.texture.create_view(&Default::default()))) }
    fn submit<I: IntoIterator<Item = wgpu::CommandBuffer>>(&self, _: &wgpu::Device, queue: &wgpu::Queue, cmds: I, _: PreviewFrame)
        -> wgpu::SubmissionIndex { queue.submit(cmds) }
}

pub struct Preview {
    player: Arc<Mutex<ruffle_core::Player>>,
    fresh: Fresh,
    texture_id: Option<egui::TextureId>,
    last_px: (u32, u32),
    last_tick: std::time::Instant,
}
impl Preview {
    pub fn new(gpu: Arc<Descriptors>, swf: &[u8], w: u32, h: u32, scale: f64) -> Result<Self, Box<dyn std::error::Error>> {
        let fresh: Fresh = Arc::new(Mutex::new(None));
        let target = PreviewTarget::create(&gpu.device, w, h, fresh.clone());
        let renderer = ruffle_render_wgpu::backend::WgpuRenderBackend::new(gpu, target)?;
        let movie = ruffle_core::tag_utils::SwfMovieData::from_data(swf, "file:///preview.swf".into(), None, None)?;
        let player = ruffle_core::PlayerBuilder::new()
            .with_renderer(renderer).with_movie(movie)
            .with_viewport_dimensions(w, h, scale).with_autoplay(true)
            .build();
        Ok(Self { player, fresh, texture_id: None, last_px: (w, h), last_tick: std::time::Instant::now() })
    }
    /// يُسجَّل مرة، ثم يُحدَّث عند تغيّر الحجم فقط: لا تسجيل في كل إطار
    pub fn sync_texture(&mut self, rs: &egui_wgpu::RenderState) {
        if let Some(view) = self.fresh.lock().unwrap().take() {
            let mut r = rs.renderer.write();
            match self.texture_id {
                None => self.texture_id = Some(r.register_native_texture(&rs.device, &view, wgpu::FilterMode::Linear)),
                Some(id) => r.update_egui_texture_from_wgpu_texture(&rs.device, &view, wgpu::FilterMode::Linear, id),
            }
        }
    }
    pub fn tick_and_render(&mut self) {
        let now = std::time::Instant::now();
        let mut p = self.player.lock().unwrap();
        p.tick(ruffle_core::FloatDuration::from_std(now - self.last_tick));
        self.last_tick = now;
        if p.needs_render() { p.render(); } // يرسم في PreviewTarget ثم queue.submit على الطابور المشترك
    }
    pub fn time_til_next_frame(&self) -> std::time::Duration { self.player.lock().unwrap().time_til_next_frame() }
    pub fn show(&mut self, ui: &mut egui::Ui) {
        let size = ui.available_size();
        let ppp = ui.ctx().pixels_per_point();
        let px = ((size.x * ppp).round().max(1.0) as u32, (size.y * ppp).round().max(1.0) as u32);
        if px != self.last_px {
            self.last_px = px;
            self.player.lock().unwrap().set_viewport_dimensions(ruffle_core::ViewportDimensions {
                width: px.0, height: px.1, scale_factor: ppp as f64 });
        }
        if let Some(id) = self.texture_id {
            ui.add(egui::Image::from_texture(egui::load::SizedTexture::new(id, size)));
        }
    }
}
// أثناء التطوير: free_texture(&id) عند إسقاط المعاينة، وإنشاء Player جديد عند كل نشر مع إعادة استعمال Descriptors.
```
