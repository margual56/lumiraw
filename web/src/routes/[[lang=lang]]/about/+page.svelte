<script>
  import Compare from '$lib/components/Compare.svelte';
  import DocPage from '$lib/components/DocPage.svelte';
  import SectionTitle from '$lib/components/SectionTitle.svelte';
  import Meta from '$lib/components/Meta.svelte';
  import { base } from '$app/paths';
  import { i18n, localized, t } from '$lib/i18n.svelte.js';

  // Rendered by tools/samples.py from photographs of our own: as recorded,
  // and as LumiRaw develops them with nothing touched.
  const SAMPLES = [
    { name: 'lilies', w: 1400, h: 933 },
    { name: 'stained-glass', w: 1400, h: 932 },
    { name: 'carved-stone', w: 1400, h: 932 },
  ];
  const src = (name, tag) => `${base}/samples/${name}-${tag}.webp`;
</script>

{#snippet sample(name, before, after, caption)}
  {@const s = SAMPLES.find((x) => x.name === name)}
  <figure>
    <Compare before={src(name, 'before')} after={src(name, 'after')} lazy
             width={s.w} height={s.h} beforeAlt={before} afterAlt={after}
             beforeLabel={t('samples.before')} afterLabel={t('samples.after')} />
    <figcaption>{caption}</figcaption>
  </figure>
{/snippet}

<Meta title={t('seo.about.title')} description={t('seo.about.description')} path="/about" />

<DocPage>
{#if i18n.locale === 'es'}
  <h1>Acerca de LumiRaw</h1>
  <p class="lede">
    LumiRaw revela fotos RAW en una pestaña del navegador. Abres el archivo que guardó la
    cámara, el programa toma las decisiones de un revelado (exposición, balance de blancos,
    contraste, ruido, corrección del objetivo) y descargas la foto terminada en JPEG, PNG o
    TIFF.
  </p>
  <p>
    El revelado se hace en tu propio dispositivo, sea un ordenador o un móvil. El programa
    está escrito en Rust y compilado a WebAssembly, y la página lo descarga una vez, como
    cualquier otro script. Ese código lee tus fotos directamente y no las envía a ningún
    sitio.
  </p>

  <SectionTitle id="why">Por qué</SectionTitle>
  <p>
    Un RAW guarda mucha más información que el JPEG que la cámara saca de él, pero
    aprovecharla suele pasar por instalar un programa grande y aprender a usarlo, o por
    subir las fotos al servidor de otro. Yo quería algo intermedio: abrir el archivo,
    obtener una foto que se parezca a lo que viste y poder cambiar cualquier decisión con la
    que no estés de acuerdo.
  </p>

  <SectionTitle id="what-it-does">Qué hace con una foto</SectionTitle>
  <p>
    Los pasos van en orden. Casi todos parten de un ajuste automático, medido sobre esa foto,
    que puedes dejar como está o cambiar:
  </p>
  <ol>
    <li><strong>Encuadre.</strong> Endereza el horizonte y las verticales que convergen si
      las detecta, y recorta.</li>
    <li><strong>Enfoque.</strong> Solo aparece cuando la foto sale blanda al medirla. Puede
      recuperar parte del desenfoque, no todo.</li>
    <li><strong>Luminosidad.</strong> Exposición medida en toda la imagen o en la zona que
      marques, y además sombras, medios tonos y luces.</li>
    <li><strong>Balance de blancos.</strong> Automático, o a partir de una zona que marques y
      que debería ser gris neutro.</li>
    <li><strong>Color y detalle.</strong> Intensidad, claridad y reducción de ruido, según
      el ruido que se mide en el archivo.</li>
    <li><strong>Local.</strong> Filtros graduados y radiales, y una herramienta para quitar
      motas de polvo.</li>
    <li><strong>Antes y después.</strong> Un deslizador sobre el resultado, con la lista de
      correcciones automáticas, que se pueden desactivar una a una.</li>
    <li><strong>Color.</strong> Siete estilos de película y de color, y encima curvas, un
      mezclador de color, LUT 3D en formato .cube, grano y viñeteado.</li>
    <li><strong>Descarga.</strong> JPEG, PNG (8 o 16 bits), TIFF o WebP, a tamaño completo o
      más pequeña, hasta 1080 px para redes sociales. Varias fotos se pueden exportar juntas
      en un zip.</li>
  </ol>
  <p>
    También hay una página para <a href={localized('/merge')}>fusionar exposiciones horquilladas</a>
    en un solo RAW con más rango dinámico antes de revelarlo.
  </p>
  <p>
    Las correcciones del objetivo (viñeteo, distorsión y aberración cromática) salen de la
    base de datos de lensfun, que tiene calibraciones de 1294 objetivos. Se aplican cuando el
    objetivo que figura en el archivo está en ella.
  </p>

  <SectionTitle id="on-real-photos">Con fotos de verdad</SectionTitle>
  <p>
    Tres fotos nuestras, tal como las grabó la cámara y tal como las revela LumiRaw sin tocar
    nada. Arrastra el separador para compararlas.
  </p>
  {@render sample('lilies', 'Lirios rojos tal como los grabó la cámara, oscuros y apagados',
    'Los mismos lirios revelados por LumiRaw, con más luz y color',
    'Lirios con una Sony α6700 y el E 16-55mm F2.8 G. Subió la exposición 2,4 pasos, corrigió un balance de blancos que tiraba mucho a rojo y quitó el viñeteo, la distorsión y la aberración cromática del objetivo.')}
  {@render sample('stained-glass', 'Una vidriera casi negra tal como la grabó la cámara',
    'La misma vidriera revelada por LumiRaw, con los colores del cristal',
    'Una vidriera a contraluz con una Sony α5000. La escena tenía 8,3 pasos de rango: la exposición subió 3,4 pasos, parte de ellos para rescatar lo que se habría quedado en negro, y el mapeo de tonos la metió en lo que puede mostrar una pantalla.')}
  {@render sample('carved-stone', 'Una piedra tallada casi a oscuras tal como la grabó la cámara',
    'La misma piedra revelada por LumiRaw, con la talla legible',
    'Una piedra tallada a 16 mm con el objetivo del kit de la α5000. Además de 2 pasos de exposición, las esquinas recuperan 2,4 pasos de viñeteo del objetivo.')}

  <SectionTitle id="your-files">Tus archivos</SectionTitle>
  <ul>
    <li>No se sube nada. Puedes comprobarlo en las herramientas de desarrollo del
      navegador: en la pestaña de red solo aparecen la página, sus scripts, el motor de
      revelado y la base de datos de objetivos. El código es público: <a href={localized('/source')}>así
      funciona y aquí está</a>.</li>
    <li>La foto no se guarda en ningún sitio. No hay cuenta ni estadísticas de uso. Lo que sí
      se conserva, solo en tu navegador, son los ajustes que hiciste a cada archivo, para que
      al volver a abrirlo estén donde los dejaste. Borrar los datos del sitio en el navegador
      los olvida.</li>
    <li>Después de la primera visita funciona sin conexión, y se puede instalar como una
      aplicación. Instalada, aparece en «Abrir con» para los archivos RAW.</li>
    <li>Los archivos exportados conservan los metadatos de la cámara: fecha de la toma,
      cámara, objetivo, exposición y la posición GPS si la cámara la guardó. Tenlo en cuenta
      antes de publicar una foto hecha en casa.</li>
  </ul>

  <SectionTitle id="not-yet">Lo que todavía no hace</SectionTitle>
  <ul>
    <li>Todo se ejecuta en un solo núcleo del procesador, así que exportar a tamaño completo
      un archivo de 26 MP tarda unos diez segundos en un portátil. La vista al 100 % tarda unos
      tres en abrirse y uno o dos después de cada cambio.</li>
    <li>Tus ajustes se quedan en el navegador donde los hiciste. Todavía no hay forma de
      llevarlos a otro dispositivo ni de guardarlos en un archivo.</li>
    <li>Un RAW grande necesita mucha memoria, y puede que un móvil antiguo no tenga
      suficiente.</li>
    <li>No hay pincel para los ajustes locales, solo filtros graduados y radiales.</li>
    <li>Los archivos X-Trans de Fujifilm y el ProRAW del iPhone están contemplados, pero solo
      se han probado con datos sintéticos, no con fotos reales.</li>
  </ul>

  <SectionTitle id="built-on">Con qué está hecho</SectionTitle>
  <ul>
    <li><a href="https://github.com/dnglab/dnglab">rawler</a>, el decodificador de RAW del
      proyecto dnglab (LGPL-2.1).</li>
    <li>La base de datos de calibraciones de <a href="https://lensfun.github.io/">lensfun</a>
      (CC BY-SA 3.0).</li>
    <li>Los estilos del paso Color, adaptados de la colección
      <a href="https://rawpedia.rawtherapee.com/Film_Simulation">Film Simulation</a> de
      RawTherapee, de Pat David, Pavlov Dmitry y Michael Ezra (CC BY-SA 4.0).</li>
    <li>El espacio de color <a href="https://bottosson.github.io/posts/oklab/">Oklab</a> de
      Björn Ottosson, donde se hacen los ajustes de luminosidad, color y tono.</li>
    <li>Rust y WebAssembly para el motor, Svelte para la página.</li>
  </ul>
{:else}
  <h1>About LumiRaw</h1>
  <p class="lede">
    LumiRaw develops raw photos in a browser tab. You open the file your camera saved, it
    makes the decisions a raw developer makes (exposure, white balance, contrast, noise, lens
    corrections), and you download a finished JPEG, PNG or TIFF.
  </p>
  <p>
    The developing happens on your own device, whether that's a computer or a phone. The
    program is written in Rust and compiled to WebAssembly, which the page downloads once
    like any other script. That code reads your photos directly and never sends them
    anywhere.
  </p>

  <SectionTitle id="why">Why</SectionTitle>
  <p>
    A raw file holds far more than the JPEG the camera makes from it, but getting at it
    usually means installing a big program and learning it, or uploading your pictures to
    someone else's server. I wanted something in between: open the file, get a picture that
    looks like what you saw, and be able to change any decision you disagree with.
  </p>

  <SectionTitle id="what-it-does">What it does with a photo</SectionTitle>
  <p>
    The steps run in order. Most of them start from an automatic setting measured on that
    photo, which you can leave alone or change:
  </p>
  <ol>
    <li><strong>Framing.</strong> Straightens a tilted horizon and converging verticals if it
      finds them, and crops.</li>
    <li><strong>Focus.</strong> Only shown when a photo measures soft. It can take back some
      of the blur, not all of it.</li>
    <li><strong>Brightness.</strong> Exposure metered on the whole frame or on an area you
      mark, plus shadows, midtones and highlights.</li>
    <li><strong>White balance.</strong> Automatic, or taken from an area you mark that should
      be neutral grey.</li>
    <li><strong>Colour and detail.</strong> Vibrance, clarity and noise reduction, set from
      the noise measured in the file.</li>
    <li><strong>Local.</strong> Graduated and radial filters, and a tool for removing dust
      spots.</li>
    <li><strong>Before and after.</strong> A slider over the result, with every automatic
      correction listed and each one can be switched off.</li>
    <li><strong>Grade.</strong> Seven film and colour looks, with tone curves, a colour mixer,
      3D LUTs in .cube format, grain and vignetting on top.</li>
    <li><strong>Download.</strong> JPEG, PNG (8 or 16-bit), TIFF or WebP, at full size or
      smaller, down to 1080 px for social media. Several photos can be exported together as a
      zip.</li>
  </ol>
  <p>
    There is also a page for <a href={localized('/merge')}>merging bracketed exposures</a> into a
    single raw frame with more dynamic range, before you develop it.
  </p>
  <p>
    Lens corrections (vignetting, distortion and chromatic aberration) come from the lensfun
    database, which has calibrations for 1,294 lenses. They're applied when the lens recorded
    in the file is in it.
  </p>

  <SectionTitle id="on-real-photos">On real photos</SectionTitle>
  <p>
    Three of our own photos, as the camera recorded them and as LumiRaw develops them with
    nothing touched. Drag the divider to compare.
  </p>
  {@render sample('lilies', 'Red lilies as the camera recorded them, dark and flat',
    'The same lilies developed by LumiRaw, brighter and in full colour',
    'Lilies on a Sony α6700 with the E 16-55mm F2.8 G. Exposure went up 2.4 stops, a white balance leaning hard towards red was corrected, and the lens\'s vignetting, distortion and chromatic aberration were taken out.')}
  {@render sample('stained-glass', 'A stained-glass window, almost black, as the camera recorded it',
    'The same window developed by LumiRaw, with the colours of the glass',
    'A stained-glass window against the light, on a Sony α5000. The scene spanned 8.3 stops: exposure went up 3.4, part of that to rescue what would otherwise have stayed black, and the tone mapping fitted it into what a screen can show.')}
  {@render sample('carved-stone', 'A carved stone, nearly dark, as the camera recorded it',
    'The same stone developed by LumiRaw, with the carving readable',
    'A carved stone at 16 mm on the α5000\'s kit lens. Besides 2 stops of exposure, the corners get back 2.4 stops the lens lost to vignetting.')}

  <SectionTitle id="your-files">Your files</SectionTitle>
  <ul>
    <li>Nothing is uploaded. You can check this in your browser's developer tools: the
      network tab shows the page, its scripts, the processing engine and the lens database,
      and nothing else. The code is public: <a href={localized('/source')}>here's how it works
      and where it is</a>.</li>
    <li>The photo isn't stored anywhere. There's no account and no analytics. What is kept,
      in your browser only, is the adjustments you made to each file, so opening it again
      brings them back. Clearing the site's data in your browser forgets them.</li>
    <li>After the first visit it works offline, and it can be installed as an app. Once
      installed, it shows up under "Open with" for raw files.</li>
    <li>Exported files keep the camera's metadata: when the photo was taken, the camera, lens
      and exposure, and the GPS position if the camera recorded one. Worth remembering before
      you post a photo taken at home.</li>
  </ul>

  <SectionTitle id="not-yet">What it doesn't do yet</SectionTitle>
  <ul>
    <li>Everything runs on a single processor core, so a full-size export of a 26 MP file
      takes about ten seconds on a laptop. The 100 % view takes about three seconds to open,
      and a second or two after each change.</li>
    <li>Your edits stay in the browser you made them in. There's no way yet to take them to
      another device or save them to a file.</li>
    <li>A large raw file needs a lot of memory, and an older phone may not have enough.</li>
    <li>There's no brush for local adjustments, only graduated and radial filters.</li>
    <li>Fujifilm X-Trans files and iPhone ProRAW are supported, but have only been tested on
      synthetic data, not on real photos.</li>
  </ul>

  <SectionTitle id="built-on">Built on</SectionTitle>
  <ul>
    <li><a href="https://github.com/dnglab/dnglab">rawler</a>, the raw decoder from the
      dnglab project (LGPL-2.1).</li>
    <li>The <a href="https://lensfun.github.io/">lensfun</a> database of lens calibrations
      (CC BY-SA 3.0).</li>
    <li>The looks on the Grade step, adapted from RawTherapee's
      <a href="https://rawpedia.rawtherapee.com/Film_Simulation">Film Simulation</a> collection
      by Pat David, Pavlov Dmitry and Michael Ezra (CC BY-SA 4.0).</li>
    <li>Björn Ottosson's <a href="https://bottosson.github.io/posts/oklab/">Oklab</a> colour
      space, which is where the lightness, colour and hue adjustments happen.</li>
    <li>Rust and WebAssembly for the engine, Svelte for the page.</li>
  </ul>
{/if}
</DocPage>
