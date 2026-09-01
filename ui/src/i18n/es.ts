/**
 * スペイン語辞書。キーの正は `ja.ts`——抜けや余りがあればコンパイルエラーになる。
 *
 * **スペイン本国のスペイン語（es-ES）で書く**（2026-09-01 の判断）。スペイン語は
 * 「どのスペイン語か」を決めないと書けない言語で、**この辞書が触る範囲でも
 * 綴りが割れる**——`vídeo`（西）/ `video`（中南米）、`Añadir` / `Agregar`、
 * `ratón` / `mouse`、`descodificar` / `decodificar`、そしてmacOSの
 * `Ajustes del Sistema`（西）/ `Configuración del Sistema`（中南米）。
 * **割れ方を裏取りしたうえで西側に寄せた**——Microsoft Store 自身が
 * es-ES に `Extensiones de vídeo HEVC`、es-419 に `Extensiones de video HEVC` を
 * 出している。金額をユーロに直すのと同じ向きで揃う。
 * 中南米版が要るようになったら **`es-419.ts` を別に足せばよい**
 * （辞書は言語ごとに1ファイルなので、この辞書を触らずに済む）。
 *
 * **語は発明せず、既にスペイン語圏で使われているものへ合わせる**（独語辞書と同じ方針）:
 *
 * - **⚑ / ✕ / U は Lightroom Classic の西語版に合わせる**——Adobeは3状態を
 *   `Con indicador` (P) / `Rechazada` (X) / `Sin indicador` (U) と呼ぶ。
 *   pictkura の P / X / U はLightroomと同じ配列なので、そのまま通じる
 * - **`indicador` を採ったことで、独語には出せない区別が付いた**。独語は
 *   ⚑ も複数選択も `Auswahl` で、同じ画面に2つの意味が並ぶ。西語は
 *   **⚑＝`indicador` 系、複数選択＝`selección` / `seleccionadas` 系**で
 *   最後まで衝突しない。**⚑側に `selección` を使わないこと**（戻すと衝突が復活する）
 * - **アプリの蔵書は `biblioteca`、写真アプリの蔵書は `fototeca`**。
 *   `fototeca` はmacOS自身の語で、独語の `Bibliothek` / `Mediathek` と同じ分け方。
 *   「ライブラリフォルダ」と「Fotosのライブラリ」が別物だと語だけで分かる
 * - **OSの用語は引いてくる**。`Papelera`（ゴミ箱）・`Ctrl`・`Mayús`（Shift）・
 *   `Espacio`・`Reproducción automática`（Windowsの自動再生）・
 *   `Ajustes del Sistema → Privacidad y seguridad`（macOS）・
 *   Microsoft Store の `Extensiones de imagen HEIF` / `Extensiones de vídeo HEVC`
 * - **金額は€に直す**（HEVCは実際 0,99 €）。英語辞書が `数百円` を
 *   "a few dollars" にしているのと同じ扱いで、直訳しない
 * - **引用符は « »、`100 %` は数字と%の間を空ける**（RAEの書き方）。
 *   釦は不定詞、見出しは名詞。二人称は tú（独語の du に合わせる）
 *
 * **「読めない」ではなく「読まない」**——`emptyRootIsPackage` /
 * `emptyManagedLibrary` / `emptyRootIsManagedLibrary` の3つは
 * `no entra a propósito en` で、故障ではなく決めごとだと言い切る。
 * **3つまとめて動かすこと**（1つだけ直すと西語が自分と食い違う）。
 * 4つ目の `emptyPhotoLibrary` だけは日英独とも「既定では」なので
 * `de forma predeterminada` にしてある——ここを揃えてはいけない。
 */
import { folderExample } from "./folderExample";
import type { Dict } from "./ja";

export const es: Dict = {
  appName: "pictkura",
  viewThumbnails: "Fotos",
  viewCalendar: "Calendario",
  searchPlaceholder: "Busca archivos, carpetas, cámaras, 2019-08 o year:2019",
  searchClear: "Borrar la búsqueda (Esc)",
  commandPalette: "Paleta de comandos",
  importFromUsb: "Importar desde USB",
  rescan: "Volver a explorar",
  size: "Tamaño",
  itemsSuffix: "elementos",
  navPlaces: "Lugares",
  navAllPhotos: "Todas las fotos",
  navFavorites: "★ Favoritos",
  navPicked: "⚑ Con indicador",
  navKinds: "Tipo",
  kindPhoto: "Fotos",
  kindRaw: "RAW",
  kindVideo: "Vídeos",
  // ショートカット一覧（`?` / `F1`）
  shortcutsTitle: "Atajos de teclado (?)",
  keyCtrl: "Ctrl",
  actionShortcuts: "Ver los atajos de teclado",
  shortcutGroups: [
    {
      title: "Cuadrícula",
      keys: [
        ["Ctrl+K / ⌘K", "Paleta de comandos (ir a una fecha o a una cámara, buscar, importar)"],
        ["Ctrl+A / ⌘A", "Seleccionar todo lo que coincide con la búsqueda y el filtro actuales"],
        ["Mayús + clic", "Seleccionar todo lo que hay entre la última foto que pulsaste y esta"],
        ["Ctrl + clic", "Añadir o quitar una foto (⌘ + clic en macOS)"],
        ["Pulsar una fecha", "Seleccionar ese día entero (púlsala otra vez para deshacerlo)"],
        ["Esc", "Dejar de seleccionar"],
      ],
    },
    {
      title: "Vista grande",
      keys: [
        ["← / →", "Foto anterior / siguiente"],
        ["P", "Marcarla con un indicador (⚑). De forma predeterminada pasa a la foto siguiente"],
        ["X", "Marcarla como rechazada (✕). Las rechazadas van a la papelera al cerrar"],
        ["U", "Quitar el indicador de esta foto (⚑ y ✕)"],
        ["Ctrl+C / ⌘C", "Copiar al portapapeles la imagen que hay en pantalla"],
        ["Ctrl+S / ⌘S", "Guardar en un archivo la imagen que hay en pantalla"],
        ["F", "Poner o quitar favorita (★)"],
        ["I", "Datos de la toma (cámara, objetivo, diafragma, ISO, GPS)"],
        ["Espacio", "Pase de diapositivas. En un vídeo, reproducir / pausar"],
        ["1 / 0", "Tamaño real 100 % / ajustar a la ventana"],
        ["F11", "Pantalla completa"],
        ["Esc", "Cerrar"],
      ],
    },
    {
      title: "Ratón (en la vista grande)",
      keys: [
        ["Doble clic", "Tamaño real 100 % ⇔ ajustar a la ventana"],
        ["Rueda", "Acercar / alejar"],
        ["Arrastrar", "Moverse por la imagen ampliada"],
        ["Clic derecho", "Abrir / abrir con / mostrar en la carpeta / mover a la papelera"],
        ["Pulsar la tira", "Ir a esa foto"],
      ],
    },
  ] as { title: string; keys: [string, string][] }[],
  navCameras: "Cámaras y dispositivos",
  navLibraryFolders: "Carpetas de la biblioteca",
  navDrives: "Unidades",
  navAddFolder: "Añadir una carpeta",
  add: "Añadir",
  browse: "Examinar…",
  addFolderPlaceholder: folderExample("p. ej. ", "usuario"),
  pickLibraryFolder: "Elige la carpeta que quieres añadir a la biblioteca",
  showMore: (n: number) => `${n} más`,
  collapse: "Ver menos",
  photosCount: (n: number) => `${n}`,
  memoriesTitle: (years: number) =>
    years === 1
      ? "Un día como hoy hace 1 año"
      : `Un día como hoy hace ${years} años`,
  viewerFavorite: "Favorita (F)",
  viewerPick: "Marcar con un indicador (P)",
  viewerUnpick: "Quitar el indicador (U)",
  viewerPicked: "Foto con indicador",
  judgeFav: "Favorita",
  judgeUnfav: "Favorita quitada",
  judgePick: "Con indicador",
  judgeUnflag: "Indicador quitado",
  viewerReject: "Marcar como rechazada (X)",
  viewerRejected: "Rechazada",
  rejectChip: (n: number) => `✕ ${n}`,
  rejectChipTitle: "Revisar las fotos rechazadas",
  rejectGateTitle: (n: number) =>
    n === 1 ? "Mover 1 foto a la papelera" : `Mover ${n} fotos a la papelera`,
  rejectGateNote:
    "Puedes recuperarlas desde la papelera (y vuelven también a la biblioteca).",
  rejectGateRestore: "Conservar",
  rejectGateBack: "Volver",
  rejectGateDiscard: "Cerrar sin borrar",
  rejectGateConfirm: (n: number) =>
    n === 1 ? "Mover 1 a la papelera" : `Mover ${n} a la papelera`,
  rejectGateTrashing: (done: number, total: number) =>
    `Moviendo… (${done} / ${total})`,
  updateFound: (v: string) => `La versión ${v} ya está disponible`,
  updateOpenPage: "Abrir la página de descarga",
  updateLater: "Más tarde",
  updateCheckNow: "Buscar actualizaciones",
  updateChecking: "Comprobando…",
  updateUpToDate: "Tienes la última versión",
  updateFailed: "No se ha podido comprobar",
  updateOnStart: "Buscar actualizaciones al arrancar",
  updateOnStartNote:
    "Le pregunta a GitHub el nombre de la última versión (una vez al día). No se envía ninguna foto ni ningún nombre de archivo. Si lo desactivas, no sale nada de este equipo, salvo cuando pulsas «Buscar actualizaciones».",
  viewerSlideshow: "Pase de diapositivas (Espacio)",
  // 抽出（Issue #13）
  extractSave: (key: string) => `Guardar esta imagen en un archivo (${key})`,
  extractCopy: (key: string) => `Copiar esta imagen al portapapeles (${key})`,
  extractSaveTitle: "Guardar la imagen como",
  extractFilter: "Imagen",
  extractSaved: "Guardada",
  extractCopied: "Copiada",
  extractFailed: "No se ha podido extraer esta imagen",
  extractSameFile: "No se puede sobrescribir el archivo original",
  viewerExif: "Datos de la foto (I)",
  viewerFullscreen: "Pantalla completa (F11)",
  viewerClose: "Cerrar (Esc)",
  viewerPrev: "Anterior (←)",
  viewerNext: "Siguiente (→)",
  viewerFitToScreen: "Ajustar a la ventana (0)",
  viewerActualSize: "Tamaño real, 100 % (1) — o doble clic",
  actualSizeBadge: "1:1",
  // 動画（第9部）
  videoUnsupported: "Este formato no se puede reproducir en la aplicación",
  videoMissing: "Este archivo no está (parece que se ha movido o se ha borrado)",
  videoCloudOnly: "Este vídeo está en la nube",
  videoCloudOnlyNote:
    "Reproducirlo aquí empieza una descarga y no se ve nada hasta que termina. Si lo abres en la aplicación predeterminada, puedes seguir el progreso de la descarga.",
  videoFailed: "No se ha podido reproducir este vídeo",
  videoOpenExternal: "Abrir en la aplicación predeterminada",
  videoCodecNote:
    "Los vídeos de los iPhone y de cámaras parecidas usan HEVC (H.265). Para reproducirlos, el sistema necesita un descodificador; en Windows es una extensión de pago del Microsoft Store (unos pocos euros).",
  videoCodecNoteMac:
    "macOS descodifica HEVC de serie, así que lo más probable es que sea un formato de grabación que no sabe manejar.",
  videoCodecNoteOther:
    "Puede que a tu sistema le falte un descodificador para este formato de grabación.",
  videoCodecHelp: "Conseguir las Extensiones de vídeo HEVC (de pago)",
  loading: "Cargando…",
  exifTitle: "Datos de la foto",
  exifCamera: "Cámara",
  exifLens: "Objetivo",
  exifAperture: "Diafragma",
  exifShutter: "Obturación",
  exifIso: "ISO",
  exifFocal: "Focal",
  exifLocation: "Ubicación",
  exifNone: "Sin datos EXIF",
  paletteInput: "Fecha, cámara, palabra o comando…",
  paletteNoResults: "Sin resultados",
  paletteGroupJumpDate: "Ir a una fecha",
  paletteGroupRecentDays: "Días recientes",
  paletteGroupCameras: "Filtrar por cámara",
  paletteGroupSearch: "Buscar",
  paletteGroupActions: "Acciones",
  paletteSearchFor: (q: string) => `Buscar «${q}»`,
  paletteSearchHint: "Nombre de archivo, carpeta, cámara",
  paletteSelect: "Seleccionar",
  paletteRun: "Ejecutar",
  paletteCloseHint: "Cerrar",
  actionShowFavorites: "Mostrar solo las favoritas",
  actionShowPicked: "Mostrar solo las que tienen indicador",
  actionShowAll: "Mostrar todas las fotos",
  actionCalendar: "Vista de calendario",
  actionThumbnails: "Cuadrícula de fotos",
  indexBuilding: "🔍 Creando el índice de búsqueda… ",
  cameraScanning: "📷 Leyendo los datos de la cámara… ",
  indexIncompleteWarning:
    "⚠ La indexación de la búsqueda se interrumpió — pueden faltar resultados (continúa en el próximo arranque)",
  indexProgressSuffix: " % — hasta que esto termine, pueden faltar resultados",
  removeRoot: (path: string) => `Quitar ${path} de la biblioteca`,
  importFrom: (path: string) => `Importar desde ${path}`,
  filterByCamera: (name: string) => `Mostrar solo las fotos hechas con ${name}`,
  jumpToYear: (year: number) => `Ir a ${year}`,
  importing: (done: number, total: number) => `Importando… ${done}/${total}`,
  importDone: (copied: number, skipped: number) =>
    `Importación terminada: ${copied} copiadas, ${skipped} omitidas`,
  importFailed: (n: number) => `, ${n} con error`,
  importIncomplete:
    " ⚠ No se han podido leer algunas carpetas — no borres la tarjeta todavía",
  syncDone: (added: number, changed: number, removed: number) =>
    `${added} añadidas, ${changed} cambiadas, ${removed} quitadas`,
  pickSource: "Elige la carpeta desde la que importar (USB / DCIM)",
  pickDestination: "Elige la carpeta de destino",
  wizardTitle: "Importar",
  wizardSources: "Origen",
  wizardOtherFolder: "Otra carpeta…",
  wizardRefresh: "Volver a explorar las unidades",
  wizardRemovable: "Extraíble",
  wizardNoDrives: "No se ha encontrado ninguna unidad",
  emptyTitle: "Todavía no hay fotos",
  emptyTitleFailed: "No se ha podido mostrar la lista",
  emptyTitleChecking: "Comprobando",
  emptyTitleStartupFailed: "La sincronización de arranque no terminó",
  emptyStartupFailed:
    "La sincronización que se ejecuta al arrancar no terminó. Puede que haya fotos que todavía no estén recogidas. Pulsa «Volver a explorar»; si no sirve, vuelve a abrir la aplicación.",
  emptyTitleMissing: "Algunos lugares no están",
  emptyTitleUnreadable: "Algunos lugares no se han podido abrir",
  emptyNoRoots:
    "Todavía no hay ninguna carpeta de la biblioteca configurada. Importa desde una tarjeta o elige una carpeta que tenga fotos dentro.",
  emptyMissing: (names: string) =>
    `Estos lugares no están: ${names}. Si es un disco externo, conéctalo y pulsa «Volver a explorar».`,
  emptyUnreadableMac: (names: string) =>
    `Estos lugares no se han podido abrir: ${names}. Dale a pictkura acceso a esa carpeta (Escritorio, Documentos, un disco externo) en Ajustes del Sistema → Privacidad y seguridad. Si está en una red, comprueba que sigue conectada y pulsa «Volver a explorar».`,
  emptyUnreadableWin: (names: string) =>
    `Estos lugares no se han podido abrir: ${names}. Comprueba los permisos de la carpeta. Si es una unidad de red, comprueba que sigue conectada y pulsa «Volver a explorar».`,
  emptyUnreadableOther: (names: string) =>
    `Estos lugares no se han podido abrir: ${names}. Comprueba que tienes permiso para leerlos y pulsa «Volver a explorar».`,
  listSeparator: ", ",
  andMore: (n: number) => `y ${n} más`,
  emptyRootIsPackage:
    "Una de las carpetas de la biblioteca es en sí una fototeca de Fotos. pictkura no entra a propósito en ese tipo de fototecas, así que nunca saldrá nada de ahí. Elige una carpeta que tenga fotos dentro o importa desde una tarjeta.",
  emptyPhotoLibrary:
    "No se ha encontrado nada aparte de la fototeca de Fotos. pictkura no lee dentro de la fototeca de Fotos de forma predeterminada: la mayoría de los originales están en iCloud, no en este Mac. Importa desde una tarjeta o elige una carpeta que tenga fotos dentro.",
  emptyManagedLibrary:
    "No se ha encontrado nada aparte de fototecas de gestores de fotos (Fotos, iPhoto o Aperture). pictkura no entra a propósito en ese tipo de fototecas: podría indexar su contenido una vez, pero no volvería a enterarse de ningún cambio posterior. Importa desde una tarjeta o elige una carpeta que tenga fotos dentro.",
  emptyRootIsManagedLibrary:
    "Una de las carpetas de la biblioteca es en sí la fototeca de un gestor de fotos (Fotos, iPhoto o Aperture). pictkura no entra a propósito en ese tipo de fototecas, así que nunca saldrá nada de ahí. Elige una carpeta que tenga fotos dentro o importa desde una tarjeta.",
  emptyAllExcluded: (names: string) =>
    `Los patrones de exclusión se saltan todo lo que se ha encontrado (por ejemplo ${names}). Puedes cambiarlos en pictkura.toml, en la carpeta de ajustes.`,
  emptyNothingHere:
    "Todavía no ha aparecido ninguna foto que pictkura pueda leer. Importa desde una tarjeta o elige una carpeta que tenga fotos dentro.",
  calendarChecking: "Comprobando…",
  emptyTitleStalled: "Algunos lugares no responden",
  emptyStalled: (names: string) =>
    `Estos lugares no responden: ${names}. Si alguno está en una red, comprueba que sigue conectada y pulsa «Volver a explorar». Si ha desaparecido para siempre, quítalo de las carpetas de la biblioteca y los demás responderán.`,
  emptyChecking:
    "Todavía se están revisando las carpetas. Si alguna está en una red, comprueba que sigue conectada y pulsa «Volver a explorar».",
  emptyLoadFailed:
    "No se ha podido cargar la lista. El motivo está en la barra de arriba. Pulsa «Volver a explorar» o vuelve a abrir la aplicación.",
  wizardPickFolderHint:
    "Elige una carpeta a la izquierda para ver las fotos que tiene",
  wizardNoImages: "No hay fotos en esta carpeta",
  wizardUnreadable: "No se ha podido leer esta carpeta (puede que se haya quitado)",
  wizardCounting: "Cargando…",
  wizardSelectAll: "Seleccionar todo",
  wizardSelectNew: "Seleccionar solo las nuevas",
  wizardClearSelection: "Quitar la selección",
  wizardSelected: (n: number) => `${n} seleccionadas`,
  wizardImportedBadge: "✓",
  wizardImportedTitle:
    "Ya importada (el mismo archivo está en la carpeta de destino)",
  wizardDestination: "Destino",
  wizardChangeDestination: "Cambiar",
  wizardStructure: "Organización",
  wizardImportButton: (n: number) => `Importar ${n}`,
  wizardImportAll: "Importar esta carpeta entera (incluidas las subcarpetas)",
  wizardImportAllShort: "Carpeta entera",
  wizardDeep: "Incluir las subcarpetas",
  wizardDeepHint:
    "Recorre todo el dispositivo para que no tengas que saber dónde están las fotos",
  wizardScanning: "Revisando el dispositivo…",
  wizardTruncated: (n: number) =>
    `Solo se muestran las primeras ${n}. Usa «Carpeta entera» para importarlo todo`,
  wizardScanIncomplete:
    "⚠ No se han podido leer algunas carpetas (puede que falten fotos)",
  decoderHeifNotice: (n: number) =>
    `⚠ ${n.toLocaleString()} fotos HEIC/HEIF no tienen miniatura aquí, y tampoco se pueden abrir. Hacen falta las Extensiones de imagen HEIF (gratis) y, además, las Extensiones de vídeo HEVC (de pago, unos pocos euros), que son las que descodifican los píxeles`,
  decoderHeifNoticeMac: (n: number) =>
    `⚠ ${n.toLocaleString()} fotos HEIC/HEIF no tienen miniatura aquí, y tampoco se pueden abrir`,
  decoderHeifNoticeOther: (n: number) =>
    `⚠ ${n.toLocaleString()} fotos HEIC/HEIF no tienen miniatura aquí, y tampoco se pueden abrir. Puede que a tu sistema le falte un descodificador para HEIC/HEVC`,
  decoderHeifHow: "Extensiones de imagen HEIF (gratis)",
  decoderHevcHow: "Extensiones de vídeo HEVC (de pago)",
  decoderNoticeDismiss: "No volver a mostrar",
  wizardOfflineTitle:
    "Este archivo está en la nube (aquí no hay vista previa; al importarlo se descargará)",
  wizardHideImported: "Ocultar las ya importadas",
  wizardAllImported:
    "Aquí no hay nada nuevo (todo lo de esta carpeta ya está importado)",
  wizardHiddenCount: (n: number) => `${n} ya importadas ocultas`,
  wizardCopying: "Importando",
  wizardEtaSeconds: (n: number) => `quedan unos ${n} s`,
  wizardEtaMinutes: (n: number) => `quedan unos ${n} min`,
  wizardEtaCalculating: "calculando el tiempo restante…",
  wizardCapped: (n: number) => `${n}+`,
  wizardMoreFiles: (n: number) => `${n} más (desplázate para cargarlas)`,
  menuOpen: "Abrir",
  menuOpenWith: (name: string) => `Abrir con ${name}`,
  menuOpenWithOther: "Abrir con otra aplicación…",
  menuReveal: "Mostrar en la carpeta",
  menuDelete: "Borrar (mover a la papelera)",
  menuFavoriteOn: "Añadir a Favoritos",
  menuFavoriteOff: "Quitar de Favoritos",
  pickEditor: "Elige una aplicación para editar",
  deleteConfirm: (n: number) =>
    n === 1
      ? "¿Mover esta foto a la papelera?"
      : `¿Mover ${n} fotos a la papelera?`,
  deleted: (n: number) => `${n} movidas a la papelera`,
  deletedSomeLeft: (n: number, left: number) =>
    `${n} movidas a la papelera (${left} no se han encontrado y se han quedado como estaban)`,
  // 複数選択と一括操作
  selectItem: "Seleccionar",
  selectedCount: (n: number) =>
    n === 1 ? "1 seleccionada" : `${n} seleccionadas`,
  selectAll: "Seleccionar todo",
  clearSelection: "Quitar la selección (Esc)",
  selectDay: "Seleccionar este día entero",
  bulkFavoriteOn: "Añadir a Favoritos",
  bulkFavoriteOff: "Quitar de Favoritos",
  bulkDelete: "Mover a la papelera",
  bulkCopy: "Copiar a una carpeta",
  bulkMove: "Mover a una carpeta",
  bulkViewer: "Ver las seleccionadas",
  pickExportFolder: "Elige la carpeta a la que exportar",
  moveConfirm: (n: number) =>
    n === 1
      ? "¿Mover esta foto a una carpeta que elegirás ahora? Sale de donde está y sale de la biblioteca (las marcas ★ y ⚑ no se llevan)."
      : `¿Mover ${n} fotos a una carpeta que elegirás ahora? Salen de donde están y salen de la biblioteca (las marcas ★ y ⚑ no se llevan).`,
  exporting: (done: number, total: number, name: string) =>
    `Exportando… ${done}/${total} ${name}`,
  exportDone: (done: number, skipped: number, failed: number, leftBehind: number) => {
    const parts = [done === 1 ? "1 foto exportada" : `${done} fotos exportadas`];
    if (skipped > 0) parts.push(`${skipped} ya estaban`);
    if (failed > 0) parts.push(`${failed} con error`);
    if (leftBehind > 0)
      parts.push(`${leftBehind} no se han podido quitar de donde estaban`);
    return parts.join(". ") + ".";
  },
  bulkPickOn: "Marcar con un indicador",
  bulkPickOff: "Quitar el indicador",
  bulkPickDone: (n: number) =>
    n === 1
      ? "1 foto marcada con un indicador"
      : `${n} fotos marcadas con un indicador`,
  bulkUnpickDone: (n: number) =>
    n === 1 ? "Indicador quitado a 1 foto" : `Indicador quitado a ${n} fotos`,
  bulkFavoriteDone: (n: number) =>
    n === 1
      ? "1 foto añadida a Favoritos"
      : `${n} fotos añadidas a Favoritos`,
  bulkUnfavoriteDone: (n: number) =>
    n === 1 ? "1 foto quitada de Favoritos" : `${n} fotos quitadas de Favoritos`,
  settings: "Ajustes",
  close: "Cerrar",
  settingsTitle: "Ajustes",
  settingsImportStructure: "Estructura de carpetas al importar",
  settingsImportStructureNote:
    "Cómo se archivan las fotos importadas según la fecha de la toma. Esto afecta a miles de archivos, y cambiarlo más adelante solo se consigue con mucho trabajo. Donde el nombre de una carpeta lleva una fecha, el año va delante en todos los idiomas, para que ordenar por nombre ordene también por tiempo.",
  settingsDestination: "Destino",
  settingsDestinationUnset: "(sin definir: lo elegirás en la primera importación)",
  settingsFlatExample: "IMG_0001.JPG (sin subcarpetas)",
  settingsCustomPattern: "Personalizado",
  settingsCustomPatternNote:
    "{year} {month} {day} se sustituyen por la fecha. Usa / para crear niveles. Los caracteres no válidos y los saltos a la carpeta superior (..) se quitan automáticamente.",
  settingsCustomPatternResult: "Carpeta resultante",
  settingsViewer: "Cuando ves una foto en grande",
  settingsAutoAdvanceToggle: "Pasar a la foto siguiente después de P / U",
  settingsAutoAdvanceNote:
    "En la vista grande, P marca la foto con ⚑ (una lista aparte de ★ Favoritos) y U le quita la marca. Con esto activado, la foto siguiente llega enseguida, así que ir marcando cuesta una tecla por foto. Con esto desactivado, te quedas en la misma foto.",
  settingsAutoplay: "Cuando conectas una unidad USB o una tarjeta SD",
  settingsAutoplayToggle: "Ofrecer pictkura en la Reproducción automática",
  settingsAutoplayNote:
    "Añade pictkura a las opciones de la Reproducción automática de Windows. Nunca arranca por su cuenta. Ten en cuenta que la entrada está escrita en japonés. Al desinstalar con el instalador se quita, pero la versión portable —y las copias de otros usuarios en el mismo PC— no están cubiertas; desactiva esto antes de quitar pictkura en esos casos.",
  settingsAbout: "Acerca de",
  settingsAboutLicense: "Publicado bajo la licencia MIT.",
  settingsManual: "Manual",
  settingsOssLicenses: "Software de código abierto que usamos",
  settingsDocNotBundled: "(no se incluye en una compilación de desarrollo)",
  settingsLanguage: "Idioma",
  settingsLanguageSystem: "El del sistema",
  settingsLanguageNote:
    "Al cambiarlo se recarga la ventana. La copia sigue en marcha en segundo plano, pero el asistente de importación se cierra y pierdes de vista su progreso, así que espera a que termine una importación antes de cambiar.",
  settingsTheme: "Aspecto",
  themeSystem: "El del sistema",
  themeLight: "Claro",
  themeDark: "Oscuro",
  settingsEditors: "Aplicaciones para editar",
  settingsEditorsNote: "Aplicaciones que has elegido en «Abrir con otra aplicación…».",
  settingsForgetEditor: "Quitar de la lista",
  calendarEmpty: "Sin fotos",
  speedPrefix: (sec: string) => `⚡ Comprobación de arranque en ${sec} s — `,
  speedUsn: "Diferencia del diario USN: ",
  speedUsnNoChange: "sin cambios, ninguna carpeta recorrida",
  speedUsnDirty: (records: number, dirs: number) =>
    `${records} entradas del diario → solo se han vuelto a explorar ${dirs} carpetas`,
  speedPruned: (skipped: number) =>
    `recorrido podado: ${skipped} carpetas omitidas`,
  speedFull: (total: number) => `recorrido completo (${total} archivos)`,
  speedNoDiff: " — sin cambios",
  speedDiff: (added: number, changed: number, removed: number) =>
    ` — ${added} añadidas, ${changed} cambiadas, ${removed} quitadas`,
};
