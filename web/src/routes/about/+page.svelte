<script>
  import { base } from '$app/paths';
  import DocPage from '$lib/components/DocPage.svelte';
  import Meta from '$lib/components/Meta.svelte';
  import { i18n, t } from '$lib/i18n.svelte.js';
</script>

<Meta title={t('seo.about.title')} description={t('seo.about.description')} path="/about" />

<DocPage>
{#if i18n.locale === 'es'}
  <h1>Qué es LumiRaw</h1>
  <p class="lede">
    LumiRaw revela fotos RAW en una pestaña del navegador. Arrastras el archivo que guardó
    la cámara, el programa toma las decisiones de un revelado (exposición, balance de
    blancos, contraste, ruido, corrección del objetivo) y descargas la foto terminada en
    JPEG, PNG o TIFF.
  </p>
  <p>
    El revelado se hace en tu ordenador. El programa está escrito en Rust y compilado a
    WebAssembly, y la página lo descarga una vez, como cualquier otro script. Tus fotos las
    lee ese código desde tu disco y no se envían a ningún sitio.
  </p>

  <h2>Por qué</h2>
  <p>
    Un RAW guarda mucha más información que el JPEG que la cámara saca de él, pero
    aprovecharla suele pasar por instalar un programa grande y aprender a usarlo, o por
    subir las fotos al servidor de otro. Yo quería algo intermedio: abrir el archivo,
    obtener una foto que se parezca a lo que viste y poder cambiar cualquier decisión con la
    que no estés de acuerdo.
  </p>

  <h2>Qué hace con una foto</h2>
  <p>
    Los pasos van en orden. Casi todos parten de un ajuste automático, medido sobre esa foto,
    que puedes dejar como está o cambiar:
  </p>
  <ol>
    <li><strong>Encuadre.</strong> Endereza el horizonte y las verticales que convergen si
      las detecta, y recorta.</li>
    <li><strong>Enfoque.</strong> Solo aparece cuando la foto sale blanda al medirla. Puede
      recuperar parte del desenfoque, no todo.</li>
    <li><strong>Brillo.</strong> Exposición medida en toda la imagen o en la zona que
      marques, y además sombras, medios tonos y luces.</li>
    <li><strong>Balance de blancos.</strong> Automático, o a partir de algo que pinches y
      que debería ser gris neutro.</li>
    <li><strong>Color y detalle.</strong> Intensidad, claridad y reducción de ruido, según
      el ruido que se mide en el archivo.</li>
    <li><strong>Local.</strong> Filtros graduados y radiales, y una herramienta para quitar
      motas de polvo.</li>
    <li><strong>Antes y después.</strong> Un deslizador sobre el resultado, con la lista de
      correcciones automáticas, que se pueden desactivar una a una.</li>
    <li><strong>Gradación.</strong> Curvas, estilos, un mezclador de color, LUT 3D en
      formato .cube, grano y viñeteado.</li>
    <li><strong>Descarga.</strong> JPEG, PNG (8 o 16 bits), TIFF o WebP, a tamaño completo o
      más pequeña. Varias fotos se pueden exportar juntas en un zip.</li>
  </ol>
  <p>
    También hay una página para <a href="{base}/merge">fusionar exposiciones horquilladas</a>
    en un solo RAW con más rango dinámico antes de revelarlo.
  </p>
  <p>
    Las correcciones del objetivo (viñeteo, distorsión y aberración cromática) salen de la
    base de datos de lensfun, que tiene calibraciones de 1294 objetivos. Se aplican cuando el
    objetivo que figura en el archivo está en ella.
  </p>

  <h2>Tus archivos</h2>
  <ul>
    <li>No se sube nada. Puedes comprobarlo en las herramientas de desarrollo del
      navegador: en la pestaña de red solo aparecen la página, sus scripts, el motor de
      revelado y la base de datos de objetivos.</li>
    <li>Tampoco se guarda nada. No hay cuenta ni estadísticas de uso. Al cerrar la pestaña,
      la foto y lo que hayas hecho con ella desaparecen.</li>
    <li>Después de la primera visita funciona sin conexión, y se puede instalar como una
      aplicación. Instalada, aparece en «Abrir con» para los archivos RAW.</li>
    <li>Los archivos exportados conservan los metadatos de la cámara: fecha de la toma,
      cámara, objetivo, exposición y la posición GPS si la cámara la guardó. Tenlo en cuenta
      antes de publicar una foto hecha en casa.</li>
  </ul>

  <h2>Lo que todavía no hace</h2>
  <ul>
    <li>Las ediciones no se guardan. Si recargas la página, se pierden.</li>
    <li>Todo se ejecuta en un solo núcleo del procesador, así que exportar a tamaño completo
      un archivo de 26 MP tarda unos diez segundos, y la vista al 100 % unos quince.</li>
    <li>No hay pincel para los ajustes locales, solo filtros graduados y radiales.</li>
    <li>Los archivos X-Trans de Fujifilm y el ProRAW del iPhone están contemplados, pero solo
      se han probado con datos sintéticos, no con fotos reales.</li>
  </ul>

  <h2>Con qué está hecho</h2>
  <ul>
    <li><a href="https://github.com/dnglab/dnglab">rawler</a>, el decodificador de RAW del
      proyecto dnglab (LGPL-2.1).</li>
    <li>La base de datos de calibraciones de <a href="https://lensfun.github.io/">lensfun</a>
      (CC BY-SA 3.0).</li>
    <li>El espacio de color <a href="https://bottosson.github.io/posts/oklab/">Oklab</a> de
      Björn Ottosson, donde se hacen los ajustes de luminosidad, color y tono.</li>
    <li>Rust y WebAssembly para el motor, Svelte para la página.</li>
  </ul>
{:else}
  <h1>About LumiRaw</h1>
  <p class="lede">
    LumiRaw develops raw photos in a browser tab. You drop in the file your camera saved, it
    makes the decisions a raw developer makes (exposure, white balance, contrast, noise, lens
    corrections), and you download a finished JPEG, PNG or TIFF.
  </p>
  <p>
    The developing happens on your own computer. The program is written in Rust and
    compiled to WebAssembly, which the page downloads once like any other script. Your
    photos are read from your disk by that code and are never sent anywhere.
  </p>

  <h2>Why</h2>
  <p>
    A raw file holds far more than the JPEG the camera makes from it, but getting at it
    usually means installing a big program and learning it, or uploading your pictures to
    someone else's server. I wanted something in between: open the file, get a picture that
    looks like what you saw, and be able to change any decision you disagree with.
  </p>

  <h2>What it does with a photo</h2>
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
    <li><strong>White balance.</strong> Automatic, or taken from something you click on that
      should be neutral grey.</li>
    <li><strong>Colour and detail.</strong> Vibrance, clarity and noise reduction, set from
      the noise measured in the file.</li>
    <li><strong>Local.</strong> Graduated and radial filters, and a tool for removing dust
      spots.</li>
    <li><strong>Before and after.</strong> A slider over the result, with every automatic
      correction listed and each one can be switched off.</li>
    <li><strong>Grade.</strong> Tone curves, looks, a colour mixer, 3D LUTs in .cube format,
      grain and vignetting.</li>
    <li><strong>Download.</strong> JPEG, PNG (8 or 16-bit), TIFF or WebP, at full size or
      smaller. Several photos can be exported together as a zip.</li>
  </ol>
  <p>
    There is also a page for <a href="{base}/merge">merging bracketed exposures</a> into a
    single raw frame with more dynamic range, before you develop it.
  </p>
  <p>
    Lens corrections (vignetting, distortion and chromatic aberration) come from the lensfun
    database, which has calibrations for 1,294 lenses. They're applied when the lens recorded
    in the file is in it.
  </p>

  <h2>Your files</h2>
  <ul>
    <li>Nothing is uploaded. You can check this in your browser's developer tools: the
      network tab shows the page, its scripts, the processing engine and the lens database,
      and nothing else.</li>
    <li>Nothing is stored either. There's no account and no analytics. Close the tab and the
      photo and your edit are gone.</li>
    <li>After the first visit it works offline, and it can be installed as an app. Once
      installed, it shows up under "Open with" for raw files.</li>
    <li>Exported files keep the camera's metadata: when the photo was taken, the camera, lens
      and exposure, and the GPS position if the camera recorded one. Worth remembering before
      you post a photo taken at home.</li>
  </ul>

  <h2>What it doesn't do yet</h2>
  <ul>
    <li>Edits aren't saved. Reloading the page loses them.</li>
    <li>Everything runs on a single processor core, so a full-size export of a 26 MP file
      takes about ten seconds, and the 100 % view about fifteen.</li>
    <li>There's no brush for local adjustments, only graduated and radial filters.</li>
    <li>Fujifilm X-Trans files and iPhone ProRAW are supported, but have only been tested on
      synthetic data, not on real photos.</li>
  </ul>

  <h2>Built on</h2>
  <ul>
    <li><a href="https://github.com/dnglab/dnglab">rawler</a>, the raw decoder from the
      dnglab project (LGPL-2.1).</li>
    <li>The <a href="https://lensfun.github.io/">lensfun</a> database of lens calibrations
      (CC BY-SA 3.0).</li>
    <li>Björn Ottosson's <a href="https://bottosson.github.io/posts/oklab/">Oklab</a> colour
      space, which is where the lightness, colour and hue adjustments happen.</li>
    <li>Rust and WebAssembly for the engine, Svelte for the page.</li>
  </ul>
{/if}
</DocPage>
