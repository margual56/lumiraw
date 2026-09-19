<script>
  import DocPage from '$lib/components/DocPage.svelte';
  import Meta from '$lib/components/Meta.svelte';
  import { i18n, localized, t } from '$lib/i18n.svelte.js';
</script>

<Meta title={t('seo.guide.title')} description={t('seo.guide.description')} path="/guide" />

<DocPage>
{#if i18n.locale === 'es'}
  <h1>Cómo pasar una foto RAW a JPEG en el navegador</h1>
  <p class="lede">
    Un archivo RAW (ARW de Sony, CR3 de Canon, NEF de Nikon, RAF de Fujifilm, DNG del móvil)
    no es todavía una foto: es lo que midió el sensor. Para enviarlo, publicarlo o imprimirlo
    hay que revelarlo, y el resultado se guarda como JPEG, PNG, TIFF o WebP. Así se hace en
    LumiRaw, sin instalar nada y sin subir el archivo a ningún servidor.
  </p>

  <h2>1. Abre el archivo</h2>
  <p>
    Arrastra el RAW a <a href={localized('/')}>la página</a>, o tócala en el móvil para
    elegirlo. Puedes abrir varios a la vez, o una carpeta entera: aparecen en una tira abajo
    y se revelan de uno en uno. El archivo se lee en tu dispositivo; no se envía a ningún
    sitio.
  </p>
  <p>
    Si solo quieres la foto con los ajustes automáticos, marca «Abrir las fotos directamente
    en Descargar» antes de abrirlas y pasa al paso 4.
  </p>

  <h2>2. Mira lo que se decidió solo</h2>
  <p>
    Al abrirla, la foto ya tiene aplicado un revelado automático: exposición, balance de
    blancos, contraste, reducción de ruido y corrección del objetivo si está en la base de
    datos. El paso «Antes / después» tiene un deslizador para compararlo con el archivo tal
    como salió de la cámara, y la lista de cada corrección, que puedes desactivar una a una.
  </p>

  <h2>3. Cambia lo que no te convenza</h2>
  <p>Los pasos van en orden y ninguno es obligatorio. Lo que más se suele tocar:</p>
  <ul>
    <li><strong>Encuadre</strong> para enderezar el horizonte o recortar.</li>
    <li><strong>Luminosidad</strong>: si la foto sale oscura o clara, marca con un recuadro lo que
      importa (una cara, por ejemplo) y la exposición se mide ahí.</li>
    <li><strong>Balance de blancos</strong>: si los colores tiran a azul o a naranja, marca
      algo que debería ser gris o blanco.</li>
    <li><strong>Color</strong> (el penúltimo paso) para dar un estilo, con curvas o un LUT .cube.</li>
  </ul>
  <p>
    Para juzgar el enfoque o el ruido, el botón 100 % muestra la foto a tamaño real. Tus
    ajustes se guardan en el navegador: si cierras la pestaña y vuelves a abrir el mismo
    archivo, siguen ahí.
  </p>

  <h2>4. Descarga en el formato que necesites</h2>
  <table>
    <thead><tr><th>Para</th><th>Formato</th><th>Por qué</th></tr></thead>
    <tbody>
      <tr><td>Enviar o publicar</td><td>JPEG</td>
        <td>Lo abre cualquier cosa. La calidad 92 que viene por defecto no se distingue del
          original y ocupa una fracción.</td></tr>
      <tr><td>Seguir editando</td><td>PNG de 16 bits</td>
        <td>Sin pérdida y con todos los tonos, para retocar después en otro programa sin
          que aparezcan escalones en los degradados.</td></tr>
      <tr><td>Una web propia</td><td>WebP</td>
        <td>Más ligero que un JPEG de la misma calidad.</td></tr>
      <tr><td>Imprenta o archivo</td><td>TIFF o PNG</td>
        <td>Sin pérdida. El TIFF es de 8 bits.</td></tr>
    </tbody>
  </table>
  <p>
    En «Lado largo», «Resolución completa» guarda todos los píxeles. Para Instagram y redes
    parecidas, 1080 px es el tamaño al que muestran una foto horizontal; si subes una más
    grande, la reducen ellos, y peor.
  </p>
  <p>
    Con varias fotos abiertas, «Revelar las N marcadas en un zip» las descarga todas juntas,
    cada una con sus propios ajustes.
  </p>

  <h2>Antes de publicar</h2>
  <p>
    El archivo descargado conserva los metadatos de la cámara, incluida la posición GPS si la
    cámara la guardó. Si la foto es de tu casa, quizá no quieras compartir eso.
  </p>

  <h2>Preguntas frecuentes</h2>
  <h3>¿Es gratis?</h3>
  <p>Sí, sin cuenta y sin límite de fotos.</p>
  <h3>¿Se suben mis fotos a algún servidor?</h3>
  <p>
    No. Todo el revelado ocurre en tu navegador; lo puedes comprobar en la pestaña de red de
    las herramientas de desarrollo.
  </p>
  <h3>¿Funciona sin conexión?</h3>
  <p>Sí, después de la primera visita. También se puede instalar como aplicación.</p>
  <h3>Mi archivo no abre</h3>
  <p>
    Si la cámara es más nueva que el decodificador, pásalo a DNG con el DNG Converter
    gratuito de Adobe y ábrelo de nuevo. La lista de <a href={localized('/cameras')}>cámaras
    compatibles</a> dice qué modelos se reconocen.
  </p>
  <h3>¿Sirve para editar un JPEG?</h3>
  <p>
    No: está hecho para archivos RAW, que guardan mucha más información que un JPEG. Un JPEG
    ya está revelado.
  </p>
  <h3>¿Funciona en el móvil?</h3>
  <p>
    Sí, incluidos los DNG del iPhone (ProRAW) y de otros teléfonos. Un RAW grande necesita
    mucha memoria, así que en un móvil antiguo puede costarle.
  </p>
{:else}
  <h1>How to convert a raw photo to JPEG in your browser</h1>
  <p class="lede">
    A raw file (Sony ARW, Canon CR3, Nikon NEF, Fujifilm RAF, DNG from a phone) isn't a photo
    yet: it's what the sensor measured. To send it, post it or print it, it has to be
    developed, and the result saved as a JPEG, PNG, TIFF or WebP. Here's how to do that in
    LumiRaw, without installing anything and without uploading the file anywhere.
  </p>

  <h2>1. Open the file</h2>
  <p>
    Drop the raw file on <a href={localized('/')}>the page</a>, or tap it on a phone to choose
    one. You can open several at once, or a whole folder: they appear in a strip along the
    bottom and are developed one at a time. The file is read on your device and isn't sent
    anywhere.
  </p>
  <p>
    If all you want is the photo with the automatic settings, tick "Open photos straight at
    Download" before opening them and skip to step 4.
  </p>

  <h2>2. See what was decided for you</h2>
  <p>
    When it opens, the photo already has an automatic development applied: exposure, white
    balance, contrast, noise reduction, and lens correction if the lens is in the database.
    The Before / after step has a slider to compare it with the file as it came off the
    camera, and a list of every correction, each of which you can switch off.
  </p>

  <h2>3. Change what you don't like</h2>
  <p>The steps go in order and none of them is required. The ones people reach for most:</p>
  <ul>
    <li><strong>Framing</strong> to straighten the horizon or crop.</li>
    <li><strong>Brightness</strong>: if the photo comes out too dark or too bright, draw a box
      over what matters (a face, say) and the exposure is metered there.</li>
    <li><strong>White balance</strong>: if the colours lean blue or orange, mark something that
      should be grey or white.</li>
    <li><strong>Grade</strong> to give it a look, with curves or a .cube LUT.</li>
  </ul>
  <p>
    To judge focus or noise, the 100 % button shows the photo at its real size. Your
    adjustments are saved in the browser: close the tab, open the same file again, and
    they're still there.
  </p>

  <h2>4. Download in the format you need</h2>
  <table>
    <thead><tr><th>For</th><th>Format</th><th>Why</th></tr></thead>
    <tbody>
      <tr><td>Sending or posting</td><td>JPEG</td>
        <td>Everything opens it. The default quality of 92 can't be told apart from the
          original and takes a fraction of the space.</td></tr>
      <tr><td>Editing further</td><td>16-bit PNG</td>
        <td>Lossless and with every tone, so you can keep editing in another program without
          banding in smooth gradients.</td></tr>
      <tr><td>Your own website</td><td>WebP</td>
        <td>Smaller than a JPEG of the same quality.</td></tr>
      <tr><td>Printing or archiving</td><td>TIFF or PNG</td>
        <td>Lossless. The TIFF is 8-bit.</td></tr>
    </tbody>
  </table>
  <p>
    Under "Long edge", "Full resolution" keeps every pixel. For Instagram and similar feeds,
    1080 px is the size they show a landscape photo at; upload a bigger one and they shrink
    it themselves, and not as well.
  </p>
  <p>
    With several photos open, "Develop all N marked, as one zip" downloads them all together,
    each with its own adjustments.
  </p>

  <h2>Before you post it</h2>
  <p>
    The downloaded file keeps the camera's metadata, including the GPS position if the camera
    recorded one. If the photo was taken at home, you may not want to share that.
  </p>

  <h2>Questions</h2>
  <h3>Is it free?</h3>
  <p>Yes, with no account and no limit on the number of photos.</p>
  <h3>Are my photos uploaded to a server?</h3>
  <p>
    No. All the developing happens in your browser; you can check in the network tab of your
    browser's developer tools.
  </p>
  <h3>Does it work offline?</h3>
  <p>Yes, after the first visit. It can also be installed as an app.</p>
  <h3>My file won't open</h3>
  <p>
    If the camera is newer than the decoder, convert the file to DNG with Adobe's free DNG
    Converter and open that. The list of <a href={localized('/cameras')}>supported cameras</a>
    says which models are recognised.
  </p>
  <h3>Can it edit a JPEG?</h3>
  <p>
    No: it's made for raw files, which hold far more than a JPEG does. A JPEG has already been
    developed.
  </p>
  <h3>Does it work on a phone?</h3>
  <p>
    Yes, including DNG files from iPhones (ProRAW) and other phones. A large raw file needs a
    lot of memory, so an older phone may struggle.
  </p>
{/if}
</DocPage>
