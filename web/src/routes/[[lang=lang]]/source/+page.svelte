<script>
  import DocPage from '$lib/components/DocPage.svelte';
  import SectionTitle from '$lib/components/SectionTitle.svelte';
  import Meta from '$lib/components/Meta.svelte';
  import { REPO } from '$lib/brand.js';
  import { i18n, localized, t } from '$lib/i18n.svelte.js';

  const link = (path) => `${REPO}/${path.endsWith('/') ? 'tree' : 'blob'}/main/${path.replace(/\/$/, '')}`;
</script>

{#snippet f(path)}<a href={link(path)}><code>{path}</code></a>{/snippet}

<Meta title={t('seo.source.title')} description={t('seo.source.description')} path="/source" />

<DocPage>
{#if i18n.locale === 'es'}
  <h1>Cómo funciona, y dónde está el código</h1>
  <p class="lede">
    Todo el código de LumiRaw es público en
    <a href={REPO}>github.com/margual56/lumiraw</a>, con licencia AGPL-3.0. Esta página sigue a
    una foto desde que la abres hasta que la descargas, y dice dónde está cada parte para que
    puedas leerla en vez de fiarte de mí.
  </p>

  <SectionTitle id="short">En resumen</SectionTitle>
  <p>
    LumiRaw es un sitio estático: HTML, JavaScript, CSS, un módulo WebAssembly y unos archivos
    de datos. No hay servidor propio, ni base de datos, ni cuentas. El servidor solo entrega
    esos archivos, igual a todo el mundo. Todo lo que se hace con una foto ocurre en tu
    navegador.
  </p>

  <SectionTitle id="what-loads">Qué descarga tu navegador</SectionTitle>
  <ul>
    <li>La página, hecha con SvelteKit y compilada a archivos estáticos ({@render f('web/')}).</li>
    <li>El motor de revelado, {@render f('web/src/lib/wasm/darkroom.wasm')}: unos 6 MB, 1,7 MB
      comprimido. Es el código Rust de {@render f('core/')} compilado a WebAssembly.</li>
    <li>La lista de cámaras de lensfun, y luego solo los objetivos de la montura de tu cámara
      ({@render f('web/src/lib/wasm/lenses/')}).</li>
    <li>Las tablas de un estilo de color, solo si lo eliges ({@render f('web/src/lib/wasm/looks/')}).</li>
  </ul>
  <p>
    No hay scripts de terceros, ni fuentes externas, ni estadísticas. El
    {@render f('web/package.json')} solo tiene herramientas de compilación: en la página no
    se ejecuta ninguna librería ajena.
  </p>

  <SectionTitle id="opening">Al abrir un archivo</SectionTitle>
  <p>
    El navegador lee el archivo en memoria ({@render f('web/src/lib/api.js')}) y se lo pasa a un
    Web Worker ({@render f('web/src/lib/wasm/worker.js')}), que lo copia a la memoria del
    módulo WebAssembly. Desde ahí, todo es Rust:
  </p>
  <ol>
    <li><strong>Decodificar.</strong> El formato de cada cámara lo lee rawler, el decodificador
      de dnglab, incluido en {@render f('core/vendor/rawler/')}. El resto del código solo habla
      con él a través de {@render f('core/src/raw.rs')}.</li>
    <li><strong>Revelar el sensor.</strong> {@render f('core/src/decode.rs')} resta el nivel de
      negro, aplica el balance de blancos de la cámara, interpola el mosaico de color (Malvar,
      He y Cutler para el patrón Bayer habitual; diferencia de color para X-Trans y otros),
      reconstruye las luces quemadas y pasa los colores de la cámara a sRGB lineal.</li>
    <li><strong>Leer los metadatos.</strong> {@render f('core/src/exif.rs')} lee el EXIF y
      {@render f('core/src/profile.rs')} deduce de él lo que importa para revelar: cuánto ruido
      cabe esperar a ese ISO, cuánto ablanda la difracción a ese diafragma, si hay riesgo de
      trepidación.</li>
    <li><strong>Buscar el objetivo.</strong> {@render f('core/src/lensdb.rs')} lo busca en la
      base de datos de lensfun.</li>
  </ol>

  <SectionTitle id="developing">El revelado</SectionTitle>
  <p>
    Cada vista previa y cada exportación pasan por estos pasos, en este orden. La mayoría está
    en {@render f('core/src/grade.rs')}, y {@render f('core/src/develop.rs')} los une:
  </p>
  <ol>
    <li><strong>Corrección del objetivo</strong>: viñeteo, distorsión y aberración cromática
      ({@render f('core/src/lensdb.rs')}).</li>
    <li><strong>Encuadre</strong>: detección de inclinación ({@render f('core/src/straighten.rs')}),
      giro y recorte ({@render f('core/src/geometry.rs')}).</li>
    <li><strong>Quitar motas</strong> ({@render f('core/src/local.rs')}), antes de medir nada.</li>
    <li><strong>Balance de blancos</strong>, aplicado como adaptación cromática de Bradford.</li>
    <li><strong>Exposición.</strong> Las mediciones en que se basan las correcciones automáticas
      están en {@render f('core/src/analyze.rs')}.</li>
    <li><strong>Mapeo de tonos local</strong>, que comprime el rango de la escena y es también
      donde actúa la claridad.</li>
    <li><strong>Filtros graduados y radiales</strong> ({@render f('core/src/local.rs')}).</li>
    <li><strong>Hombro fílmico</strong>, que lleva la luz de la escena a lo que cabe en una
      pantalla.</li>
    <li><strong>En Oklab</strong>: punto negro y blanco, contraste, sombras/medios/luces,
      ruido de color y de luminancia, intensidad, recuperación de enfoque y nitidez.</li>
    <li><strong>Color</strong>: mezclador ({@render f('core/src/mixer.rs')}), blanco y negro y
      efectos ({@render f('core/src/effects.rs')}), el estilo ({@render f('core/src/looks.rs')},
      {@render f('core/src/lut.rs')}), tus curvas ({@render f('core/src/curve.rs')}), tu .cube,
      viñeteo y grano.</li>
  </ol>
  <p>
    Cada paso automático anota lo que decidió en un informe
    ({@render f('core/kit/src/report.rs')}). Eso es lo que la interfaz enseña junto a cada
    control y en la lista de correcciones del paso Antes y después.
  </p>
  <p>
    Las vistas previas se revelan a tamaño reducido. La vista al 100 % revela solo el trozo
    que ves, con las decisiones tomadas sobre la foto entera, para que coincida con la
    exportación.
  </p>

  <SectionTitle id="exporting">Al exportar</SectionTitle>
  <ul>
    <li>PNG, JPEG y TIFF se codifican en Rust ({@render f('core/src/output.rs')}), con los
      metadatos de la cámara copiados ({@render f('core/src/exif.rs')}).</li>
    <li>WebP lo codifica el propio navegador desde un canvas, y
      {@render f('web/src/lib/wasm/webp.js')} le añade el EXIF.</li>
    <li>El zip de varias fotos lo escribe {@render f('web/src/lib/zip.js')}, unas cien líneas
      sin librerías.</li>
    <li>La descarga es un enlace a un blob creado en la propia página: el archivo va de la
      memoria de la pestaña a tu disco.</li>
  </ul>

  <SectionTitle id="merging">Fusionar exposiciones</SectionTitle>
  <p>
    {@render f('core/src/merge.rs')} agrupa las fotos en horquillados según los ajustes de la
    cámara y el tiempo entre disparos, comprueba la exposición de cada una con sus propios
    píxeles, las alinea (con mapas de bits por umbral de mediana), las combina dando a cada
    zona el peso de la exposición que mejor la recoge y quita los objetos que se movieron. La
    API está en {@render f('core/darkroom/src/bracket.rs')}.
  </p>

  <SectionTitle id="stored">Qué se guarda</SectionTitle>
  <ul>
    <li>En el <code>localStorage</code> del navegador: los ajustes de cada archivo, identificado
      por nombre, tamaño y fecha de modificación, hasta 300 archivos
      ({@render f('web/src/lib/memory.js')}); el idioma; y si quieres enderezar
      automáticamente.</li>
    <li>En la caché del service worker ({@render f('web/src/service-worker.js')}): los archivos
      de la propia aplicación, para que funcione sin conexión.</li>
    <li>Ni un píxel de tus fotos, en ningún sitio.</li>
  </ul>

  <SectionTitle id="checking">Comprobarlo tú</SectionTitle>
  <ul>
    <li><strong>Sin conexión.</strong> Carga la página, activa el modo avión y abre un RAW.
      Funciona igual.</li>
    <li><strong>Pestaña de red.</strong> En las herramientas de desarrollo, revela y exporta una
      foto: no aparece ninguna petición con su contenido.</li>
    <li><strong>Política de seguridad.</strong> El sitio manda una cabecera
      Content-Security-Policy ({@render f('web/static/_headers')}) que prohíbe a la página
      conectarse a cualquier dirección que no sea la suya. La hace cumplir tu navegador, no
      yo, y puedes verla en las cabeceras de la respuesta.</li>
    <li><strong>Ejecutarlo tú.</strong> Si no quieres fiarte de mi servidor, no hace falta:
      clona el repositorio, ejecuta <code>npm run build</code> y luego
      <code>npm run preview</code>, y lo tendrás en <code>localhost:8790</code>.
      {@render f('tools/build.sh')} recompila también el módulo WebAssembly si tienes Rust.</li>
  </ul>

  <SectionTitle id="map">Mapa del repositorio</SectionTitle>
  <table>
    <thead><tr><th>Carpeta</th><th>Qué hay</th></tr></thead>
    <tbody>
      <tr><td>{@render f('core/src/')}</td><td>El proceso de revelado, en Rust.</td></tr>
      <tr><td>{@render f('core/kit/')}</td><td>Imágenes, color, matrices, filtros y matemáticas.</td></tr>
      <tr><td>{@render f('core/darkroom/')}</td><td>La API sobre el proceso, y el módulo WebAssembly que se compila de ella.</td></tr>
      <tr><td>{@render f('core/vendor/rawler/')}</td><td>El decodificador de RAW, incluido con un cambio en su manifiesto.</td></tr>
      <tr><td>{@render f('web/src/')}</td><td>La página: pasos, componentes, traducciones y el worker.</td></tr>
      <tr><td>{@render f('web/static/')}</td><td>Iconos, cabeceras, redirecciones y las fotos de ejemplo.</td></tr>
      <tr><td>{@render f('tools/')}</td><td>Compilación, la base de datos de objetivos, los estilos y pruebas.</td></tr>
      <tr><td>{@render f('.github/workflows/')}</td><td>Las pruebas que se ejecutan con cada cambio.</td></tr>
    </tbody>
  </table>

  <SectionTitle id="licence">Licencia</SectionTitle>
  <p>
    LumiRaw es AGPL-3.0 ({@render f('LICENSE')}): puedes leerlo, modificarlo y publicarlo, y
    si ofreces una versión modificada a otros tienes que compartir su código. rawler es
    LGPL-2.1, la base de datos de lensfun CC BY-SA 3.0 y los estilos CC BY-SA 4.0; más detalles
    en <a href={localized('/about#built-on')}>Acerca de</a>.
  </p>
{:else}
  <h1>How it works, and where the code is</h1>
  <p class="lede">
    All of LumiRaw's code is public at <a href={REPO}>github.com/margual56/lumiraw</a>, under
    the AGPL-3.0. This page follows a photo from the moment you open it to the moment you
    download it, and says where each part lives, so you can read it instead of taking my word
    for it.
  </p>

  <SectionTitle id="short">The short version</SectionTitle>
  <p>
    LumiRaw is a static site: HTML, JavaScript, CSS, a WebAssembly module and a few data files.
    There's no server of its own, no database and no accounts. The server only hands out those
    files, the same ones to everyone. Everything that happens to a photo happens in your
    browser.
  </p>

  <SectionTitle id="what-loads">What your browser downloads</SectionTitle>
  <ul>
    <li>The page, built with SvelteKit into static files ({@render f('web/')}).</li>
    <li>The engine, {@render f('web/src/lib/wasm/darkroom.wasm')}: about 6 MB, 1.7 MB
      compressed. It's the Rust code in {@render f('core/')} compiled to WebAssembly.</li>
    <li>lensfun's list of cameras, then only the lenses for your camera's mount
      ({@render f('web/src/lib/wasm/lenses/')}).</li>
    <li>A look's tables, only if you pick that look ({@render f('web/src/lib/wasm/looks/')}).</li>
  </ul>
  <p>
    No third-party scripts, no external fonts, no analytics. {@render f('web/package.json')}
    only lists build tools: no outside library runs on the page.
  </p>

  <SectionTitle id="opening">Opening a file</SectionTitle>
  <p>
    The browser reads the file into memory ({@render f('web/src/lib/api.js')}) and hands it to a
    Web Worker ({@render f('web/src/lib/wasm/worker.js')}), which copies it into the WebAssembly
    module's memory. From there on it's all Rust:
  </p>
  <ol>
    <li><strong>Decoding.</strong> Each camera's format is read by rawler, dnglab's raw decoder,
      vendored in {@render f('core/vendor/rawler/')}. The rest of the code only talks to it
      through {@render f('core/src/raw.rs')}.</li>
    <li><strong>Developing the sensor data.</strong> {@render f('core/src/decode.rs')} subtracts
      the black level, applies the camera's white balance, demosaics (Malvar, He and Cutler for
      the usual Bayer pattern; colour difference for X-Trans and others), rebuilds blown
      highlights and converts the camera's colours to linear sRGB.</li>
    <li><strong>Reading the metadata.</strong> {@render f('core/src/exif.rs')} reads the EXIF,
      and {@render f('core/src/profile.rs')} works out what matters for developing: how much noise
      to expect at that ISO, how much diffraction softens that aperture, whether camera shake
      is likely.</li>
    <li><strong>Finding the lens.</strong> {@render f('core/src/lensdb.rs')} looks it up in the
      lensfun database.</li>
  </ol>

  <SectionTitle id="developing">Developing</SectionTitle>
  <p>
    Every preview and every export goes through these steps, in this order. Most of them are in
    {@render f('core/src/grade.rs')}, and {@render f('core/src/develop.rs')} ties them
    together:
  </p>
  <ol>
    <li><strong>Lens corrections</strong>: vignetting, distortion and chromatic aberration
      ({@render f('core/src/lensdb.rs')}).</li>
    <li><strong>Framing</strong>: tilt detection ({@render f('core/src/straighten.rs')}),
      rotation and crop ({@render f('core/src/geometry.rs')}).</li>
    <li><strong>Dust spot removal</strong> ({@render f('core/src/local.rs')}), before anything
      is measured.</li>
    <li><strong>White balance</strong>, applied as a Bradford chromatic adaptation.</li>
    <li><strong>Exposure.</strong> The measurements the automatic corrections work from are in
      {@render f('core/src/analyze.rs')}.</li>
    <li><strong>Local tone mapping</strong>, which compresses the scene's range and is also
      where clarity comes in.</li>
    <li><strong>Graduated and radial filters</strong> ({@render f('core/src/local.rs')}).</li>
    <li><strong>A filmic shoulder</strong>, which brings scene light into what a screen can
      show.</li>
    <li><strong>In Oklab</strong>: black and white point, contrast, shadows/midtones/highlights,
      colour and luminance noise, vibrance, focus recovery and sharpening.</li>
    <li><strong>Grade</strong>: the colour mixer ({@render f('core/src/mixer.rs')}), black and
      white and effects ({@render f('core/src/effects.rs')}), the look
      ({@render f('core/src/looks.rs')}, {@render f('core/src/lut.rs')}), your curves
      ({@render f('core/src/curve.rs')}), your .cube, vignette and grain.</li>
  </ol>
  <p>
    Each automatic step writes down what it decided in a report
    ({@render f('core/kit/src/report.rs')}). That's what the interface shows next to each
    control and in the list of corrections on the Before and after step.
  </p>
  <p>
    Previews are developed small. The 100 % view develops only the piece you're looking at,
    using the decisions made on the whole photo, so it matches the export.
  </p>

  <SectionTitle id="exporting">Exporting</SectionTitle>
  <ul>
    <li>PNG, JPEG and TIFF are encoded in Rust ({@render f('core/src/output.rs')}), with the
      camera's metadata copied over ({@render f('core/src/exif.rs')}).</li>
    <li>WebP is encoded by the browser itself from a canvas, and
      {@render f('web/src/lib/wasm/webp.js')} adds the EXIF to it.</li>
    <li>The zip for several photos is written by {@render f('web/src/lib/zip.js')}, about a
      hundred lines with no libraries.</li>
    <li>The download is a link to a blob made by the page itself: the file goes from the tab's
      memory to your disk.</li>
  </ul>

  <SectionTitle id="merging">Merging brackets</SectionTitle>
  <p>
    {@render f('core/src/merge.rs')} groups photos into brackets by camera settings and the time
    between shots, checks each one's exposure against its own pixels, aligns them (with median
    threshold bitmaps), blends them so each area comes from the exposure that recorded it best,
    and takes out things that moved. The API for it is in
    {@render f('core/darkroom/src/bracket.rs')}.
  </p>

  <SectionTitle id="stored">What's stored</SectionTitle>
  <ul>
    <li>In your browser's <code>localStorage</code>: the settings for each file, identified by
      name, size and modification date, up to 300 files ({@render f('web/src/lib/memory.js')});
      the language; and whether to straighten automatically.</li>
    <li>In the service worker's cache ({@render f('web/src/service-worker.js')}): the app's own
      files, so it works offline.</li>
    <li>Not a single pixel of your photos, anywhere.</li>
  </ul>

  <SectionTitle id="checking">Checking it yourself</SectionTitle>
  <ul>
    <li><strong>Offline.</strong> Load the page, switch on airplane mode and open a raw file.
      It works the same.</li>
    <li><strong>Network tab.</strong> In the developer tools, develop and export a photo: no
      request carries its contents.</li>
    <li><strong>Security policy.</strong> The site sends a Content-Security-Policy header
      ({@render f('web/static/_headers')}) that forbids the page from connecting to any address
      but its own. Your browser enforces it, not me, and you can see it in the response
      headers.</li>
    <li><strong>Run it yourself.</strong> If you'd rather not trust my server, you don't have
      to: clone the repository, run <code>npm run build</code> and then
      <code>npm run preview</code>, and it's on <code>localhost:8790</code>.
      {@render f('tools/build.sh')} also rebuilds the WebAssembly module if you have Rust.</li>
  </ul>

  <SectionTitle id="map">Map of the repository</SectionTitle>
  <table>
    <thead><tr><th>Folder</th><th>What's in it</th></tr></thead>
    <tbody>
      <tr><td>{@render f('core/src/')}</td><td>The developing pipeline, in Rust.</td></tr>
      <tr><td>{@render f('core/kit/')}</td><td>Images, colour, matrices, filters and maths.</td></tr>
      <tr><td>{@render f('core/darkroom/')}</td><td>The API over the pipeline, and the WebAssembly module built from it.</td></tr>
      <tr><td>{@render f('core/vendor/rawler/')}</td><td>The raw decoder, vendored with one change to its manifest.</td></tr>
      <tr><td>{@render f('web/src/')}</td><td>The page: steps, components, translations and the worker.</td></tr>
      <tr><td>{@render f('web/static/')}</td><td>Icons, headers, redirects and the sample photos.</td></tr>
      <tr><td>{@render f('tools/')}</td><td>Build, the lens database, the looks, and tests.</td></tr>
      <tr><td>{@render f('.github/workflows/')}</td><td>The checks that run on every change.</td></tr>
    </tbody>
  </table>

  <SectionTitle id="licence">Licence</SectionTitle>
  <p>
    LumiRaw is AGPL-3.0 ({@render f('LICENSE')}): you can read it, change it and publish it, and
    if you offer a modified version to others you have to share its code. rawler is LGPL-2.1,
    the lensfun database CC BY-SA 3.0 and the looks CC BY-SA 4.0; more on the
    <a href={localized('/about#built-on')}>About</a> page.
  </p>
{/if}
</DocPage>
