<script>
  import DocPage from '$lib/components/DocPage.svelte';
  import Meta from '$lib/components/Meta.svelte';
  import { i18n, t } from '$lib/i18n.svelte.js';

  // Counted from the camera definitions in core/vendor/rawler/data/cameras,
  // one file per model. Recount when the decoder is updated.
  const MAKERS = [
    ['Canon', 143, 'CR3, CR2, CRW', 'EOS R5 Mark II, EOS R6 Mark III, EOS R8, EOS R50, EOS 5D Mark IV, EOS 90D'],
    ['Sony', 107, 'ARW, SR2, SRF', 'α1, α7 IV, α7R V, α7C II, α6700, ZV-E10 II'],
    ['Panasonic', 93, 'RW2, RAW', 'Lumix S5 II, G9 II, GH5S, G90, LX5'],
    ['Nikon', 91, 'NEF, NRW', 'Z 9, Z 8, Z f, Z 6III, Z 50, D850'],
    ['Fujifilm', 86, 'RAF', 'X-T5, X100VI, X-H2S, X-S20, GFX 100 II'],
    ['Olympus / OM System', 72, 'ORF', 'OM-1 Mark II, OM-5, E-M1 Mark II, E-M10'],
    ['Pentax', 30, 'PEF, DNG', 'K-3 Mark III, KP, K-70, KF'],
    ['Leica', 27, 'RWL, DNG', 'Q2, CL, M Monochrom (Typ 246), D-Lux 5'],
    ['Samsung', 21, 'SRW', 'NX1, NX500'],
    ['Phase One', 16, 'IIQ', 'IQ3 100MP'],
    ['Kodak', 16, 'DCR, KDC', ''],
    ['Hasselblad', 11, '3FR, FFF', ''],
    ['Minolta', 10, 'MRW', ''],
    ['Leaf', 6, 'MOS', ''],
    ['ARRI', 4, 'ARI', ''],
    ['Epson', 3, 'ERF', ''],
    ['Mamiya', 1, 'MEF', ''],
  ];
</script>

<Meta title={t('seo.cameras.title')} description={t('seo.cameras.description')} path="/cameras" />

<DocPage>
{#if i18n.locale === 'es'}
  <h1>Cámaras y formatos RAW compatibles</h1>
  <p class="lede">
    LumiRaw lee los RAW con rawler, el decodificador del proyecto dnglab. Tiene datos de color
    para más de 700 modelos de cámara y abre archivos DNG de cualquier cámara o móvil.
  </p>

  <h2>Por marca</h2>
{:else}
  <h1>Supported cameras and raw formats</h1>
  <p class="lede">
    LumiRaw reads raw files with rawler, the decoder from the dnglab project. It has colour
    data for more than 700 camera models, and it opens DNG files from any camera or phone.
  </p>

  <h2>By maker</h2>
{/if}

<table>
  <thead>
    <tr>
      <th>{t('cameras.maker')}</th>
      <th>{t('cameras.models')}</th>
      <th>{t('cameras.files')}</th>
      <th>{t('cameras.examples')}</th>
    </tr>
  </thead>
  <tbody>
    {#each MAKERS as [maker, count, files, examples]}
      <tr><td>{maker}</td><td>{count}</td><td>{files}</td><td>{examples}</td></tr>
    {/each}
  </tbody>
</table>

{#if i18n.locale === 'es'}
  <p class="aside">
    Las cuentas incluyen modelos antiguos y variantes de un mismo modelo. Apple también figura,
    con las QuickTake de los noventa; los iPhone guardan DNG, que va aparte.
  </p>

  <h2>DNG</h2>
  <p>
    Un DNG lleva sus propios datos de color, así que no hace falta que la cámara esté en la
    lista. Eso incluye los DNG que graban directamente cámaras como Pentax, Leica o Ricoh, los
    que produce el DNG Converter de Adobe y los de móviles: ProRAW del iPhone (también los que
    usan compresión JPEG XL), Pixel y Galaxy.
  </p>
  <p>
    Algunos DNG ya vienen desmosaicados, como ProRAW o las fusiones HDR de otros programas.
    Se revelan igual que el resto, con su matriz de color y su balance de blancos.
  </p>

  <h2>Fujifilm X-Trans</h2>
  <p>
    Los sensores X-Trans no usan el patrón Bayer de casi todas las demás cámaras, sino una
    matriz de 6×6. LumiRaw tiene un desmosaicado propio para ellos; aplicar el de Bayer llena
    la imagen de color falso.
  </p>

  <h2>Si tu archivo no abre</h2>
  <p>
    La página dice por qué: una cámara que el decodificador no conoce o un archivo dañado (lo
    más habitual es que se cortara al copiarlo de la tarjeta). Una cámara que salió después de
    esta versión del decodificador puede no estar aún. Mientras tanto, convertir el archivo a
    DNG con el DNG Converter gratuito de Adobe lo soluciona, porque el DNG lleva sus propios
    datos de color.
  </p>
{:else}
  <p class="aside">
    Counts include older models and variants of the same body. Apple is on the list too, for
    the QuickTake cameras from the nineties; iPhones save DNG, which is covered below.
  </p>

  <h2>DNG</h2>
  <p>
    A DNG carries its own colour data, so the camera doesn't need to be on the list. That
    covers DNGs written in camera by Pentax, Leica or Ricoh, files from Adobe's DNG Converter,
    and phones: iPhone ProRAW (including the JPEG XL compressed kind), Pixel and Galaxy.
  </p>
  <p>
    Some DNGs are already demosaiced, like ProRAW or HDR merges from other programs. They're
    developed the same way as everything else, with their colour matrix and white balance.
  </p>

  <h2>Fujifilm X-Trans</h2>
  <p>
    X-Trans sensors don't use the Bayer pattern almost every other camera has, but a 6×6
    arrangement. LumiRaw has a separate demosaic for them; running the Bayer one on an X-Trans
    file fills it with false colour.
  </p>

  <h2>If your file won't open</h2>
  <p>
    The page tells you why: a camera the decoder doesn't know, or a damaged file (usually one
    that got cut short while copying off the card). A camera released after this version of
    the decoder may not be in it yet. Until it is, converting the file to DNG with Adobe's free
    DNG Converter works around it, because a DNG carries its own colour data.
  </p>
{/if}
</DocPage>
