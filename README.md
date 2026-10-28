# Al-Jazari

An open authoring tool for 2D animation in the spirit of Macromedia Flash 8 and
ActionScript 2, adding the camera and layer depth of Adobe Animate and skeletal
animation. Written in Rust and built on [Ruffle](https://ruffle.rs).

**Status:** planning. There is no usable code yet.

Named after Ismail al-Jazari, the 12th-century engineer whose *Book of Knowledge
of Ingenious Mechanical Devices* (1206) describes automata: figures set in motion
by water, cams and gears.

## Goals

- Open and save XFL, the Flash CS5–CS6 and Animate format. Legacy binary FLA
  files are converted with [JPEXS FLA Convert](https://github.com/jindrapetrik/jpexs-fla-convert).
- Compile ActionScript 2 with [Rascal](https://github.com/Dinnerbone/rascal) and
  publish SWF version 8.
- Preview in an embedded Ruffle player, with a setting to render on the GPU or
  the CPU.
- Animate-style camera and layer depth, baked into ordinary frames so they work
  with ActionScript 2 and Flash Player 8.
- Bones and inverse kinematics, also baked at publish time.
- Arabic text that is shaped and laid out right to left, both in the editor and
  in published SWF files.
- Windows and Linux (Ubuntu, Linux Mint).

The interface is in English for now; an Arabic interface will follow.

## License

Released into the public domain under the [Unlicense](LICENSE). Third-party code
included in this repository or in its builds keeps its own license.

---

<div dir="rtl">

# الجزري

أداة مفتوحة لصناعة الرسوم المتحركة ثنائية الأبعاد على نهج ماكروميديا فلاش 8
وأكشن سكريبت 2، تضيف إليه كاميرا أنيميت وعمق طبقاتها والتحريك بالهيكل العظمي.
مكتوبة بلغة Rust ومبنية على Ruffle.

**الحالة:** في طور التخطيط، ولا شيفرة صالحة للاستعمال بعد.

سُمّيت باسم بديع الزمان الجزري، مهندس القرن السادس الهجري، صاحب كتاب «الجامع بين
العلم والعمل النافع في صناعة الحيل» الذي وصف فيه آلات ذاتية الحركة تديرها المياه
والكامات والتروس.

**الرخصة:** ملك عام بموجب Unlicense، وما يُضمَّن فيها من شيفرات الآخرين يبقى على
رخصته.

</div>
