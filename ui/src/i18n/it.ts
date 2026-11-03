/**
 * イタリア語辞書。キーの正は `ja.ts`——抜けや余りがあればコンパイルエラーになる。
 *
 * **語は発明せず、既にイタリア語圏で使われているものへ合わせる**（独語・西語辞書と同じ方針）:
 *
 * - **⚑ / ✕ / U は Lightroom Classic の伊語版に合わせる**——Adobe の伊語ヘルプは
 *   3状態を `Contrassegnate`（`Con contrassegno`）/ `Rifiutate` / `Senza contrassegno`
 *   と呼ぶ（スマートコレクション・メタデータフィルターの頁）。pictkura の P / X / U は
 *   Lightroom と同じ配列なので、そのまま通じる
 * - **⚑＝`contrassegno` 系、複数選択＝`selezione` / `selezionate` 系**。西語の
 *   `indicador` / `selección` と同じ分け方で、同じ画面に2つの意味が並ばない。
 *   **⚑側に `selezione` を使わないこと**
 * - **⚑ と ✕ の両方に出る文字列には `contrassegno` を使わない**。`judgeUnflag` と
 *   `U` のショートカット行は ✕ を取り消したときにも出るので、**両方を覆う `segno`**
 *   にしてある（西語の `marca` と同じ理由）。`segno` は ★ / ⚑ の印全般にも使う
 * - **ライブラリは `libreria`**。Apple の伊語は写真.app の蔵書も `libreria di Foto`
 *   と呼ぶので、西語の `fototeca` のような別語が無い。**写真.app 側は必ず
 *   `di Foto` / `di un'app di gestione foto` を付けて区別する**
 * - **OSの用語は引いてくる**。`Cestino`（ゴミ箱。Windows・macOS とも）・`Ctrl`・
 *   `Maiusc`（Shift）・`Barra spaziatrice`（Space）・`Appunti`（クリップボード）・
 *   `Esplora file`・`Mostra altre opzioni`（Windows 11 のメニュー）・
 *   `AutoPlay`（Windows の自動再生。Windows 11 の伊語版は英語名のまま）・
 *   `Impostazioni di Sistema → Privacy e sicurezza`（macOS）・
 *   Microsoft Store の `Estensioni immagini HEIF` / `Estensioni video HEVC`
 *   （2026-11-03、apps.microsoft.com の it-IT 表示で確かめた）。
 *   **製品名に冠詞を付けない**（`di Microsoft Store`。西語と同じ）
 * - **金額は€に直す**（`pochi euro`）。英語が `数百円` を "a few dollars" にしているのと同じ扱い
 * - **二人称は tu**。Apple・Microsoft の伊語UIとも tu の命令形で書いている
 *   （`Scegli`・`Fai clic`）。**釦とメニューも tu の命令形**（`Annulla`・`Elimina`・
 *   `Apri`——Apple/Microsoft の伊語の釦と同じ形）、見出しは名詞
 * - **引用符は “ ”**（Apple の伊語UIの書き方）。`100%` は数字と%を離さない
 * - `foto` / `video` / `file` は単複同形。**数で変わるのは冠詞・形容詞・分詞・動詞**
 *   なので、`one()` はそちらに掛ける（`1 foto spostata` / `2 foto spostate`）。
 *   写真を指す分詞は女性形、`elemento` を指すものは男性形
 *
 * **「読めない」ではなく「読まない」**——`emptyRootIsPackage` /
 * `emptyManagedLibrary` / `emptyRootIsManagedLibrary` の3つは `di proposito` で
 * 決めごとだと言い切る。`emptyPhotoLibrary` だけは「既定では」
 * （`Per impostazione predefinita`）——ここを揃えてはいけない。
 */
import { folderExample } from "./folderExample.ts";
import { num, one } from "./plural.ts";
import type { Dict } from "./ja.ts";

export const it: Dict = {
  appName: "pictkura",
  viewThumbnails: "Foto",
  viewCalendar: "Calendario",
  searchPlaceholder: "Cerca file, cartelle, fotocamere, 2019-08 o year:2019",
  searchClear: "Cancella la ricerca (Esc)",
  commandPalette: "Palette dei comandi",
  importFromUsb: "Importa da USB",
  rescan: "Ripeti scansione",
  size: "Dimensioni",
  /**
   * 一覧の件数。**数と単位を1つのキーにする**（2026-09-02、ゲート2の指摘）。
   * 単位だけのキーでは、どの言語も単数形を書けない。
   */
  itemsCount: (n: number) => `${num(n)} ${one(n, "elemento", "elementi")}`,
  navPlaces: "Posizioni",
  navAllPhotos: "Tutte le foto",
  navFavorites: "★ Preferiti",
  navPicked: "⚑ Contrassegnate",
  navOutgoing: "In uscita",
  outgoingGuide: "Nell'app del tuo servizio di foto, scegli questa cartella In uscita come cartella di cui eseguire il backup (da sincronizzare). Da quel momento, a ogni importazione le foto vengono messe qui e il servizio le raccoglie. Se usi Google Foto: aggiungi questa cartella al backup delle cartelle di Google Foto.",
  navKinds: "Tipo",
  kindPhoto: "Foto",
  kindRaw: "RAW",
  kindVideo: "Video",
  // ショートカット一覧（`?` / `F1`）
  shortcutsTitle: "Scorciatoie da tastiera (?)",
  keyCtrl: "Ctrl",
  actionShortcuts: "Mostra le scorciatoie da tastiera",
  shortcutGroups: [
    {
      title: "Griglia",
      keys: [
        ["Ctrl+K / ⌘K", "Palette dei comandi (vai a una data o a una fotocamera, cerca, importa)"],
        ["Ctrl+, / ⌘,", "Apri le impostazioni (come l'ingranaggio nella barra degli strumenti)"],
        ["Ctrl+A / ⌘A", "Seleziona tutto ciò che corrisponde alla ricerca e ai filtri attuali"],
        ["Maiusc + clic", "Seleziona tutto tra l'ultima foto su cui hai fatto clic e questa"],
        ["Ctrl + clic", "Aggiungi o togli una foto (⌘ + clic su macOS)"],
        ["Fai clic su una data", "Seleziona tutto quel giorno (fai di nuovo clic per annullare)"],
        ["Esc", "Smetti di selezionare"],
      ],
    },
    {
      title: "Vista grande",
      keys: [
        ["← / →", "Foto precedente / successiva"],
        ["P", "Contrassegnala (⚑). Per impostazione predefinita passa alla foto successiva"],
        ["X", "Rifiutala (✕). Le foto rifiutate vanno nel Cestino quando chiudi la vista"],
        ["U", "Togli il segno da questa foto (⚑ o ✕)"],
        ["Ctrl+C / ⌘C", "Copia negli Appunti l'immagine sullo schermo"],
        ["Ctrl+S / ⌘S", "Salva in un file l'immagine sullo schermo"],
        ["Ctrl+P / ⌘P", "Stampa la foto sullo schermo"],
        ["F", "Aggiungi o togli dai preferiti (★)"],
        ["I", "Dati del file e dello scatto (fotocamera, obiettivo, diaframma, tempo di posa, ISO, lunghezza focale, GPS)"],
        ["Barra spaziatrice", "Presentazione. Su un video, riproduci / metti in pausa"],
        ["1 / 0", "1 alterna tra dimensioni reali (100%) e adattamento allo schermo; 0 adatta sempre"],
        ["F11", "Schermo intero"],
        ["Esc", "Chiudi"],
      ],
    },
    {
      title: "Mouse (nella vista grande)",
      keys: [
        ["Doppio clic", "Dimensioni reali 100% ⇔ adatta allo schermo"],
        ["Rotellina", "Ingrandisci / riduci"],
        ["Trascina", "Spostati nell'immagine ingrandita"],
        ["Clic destro", "Menu (apri, copia, ruota, stampa, informazioni, elimina e altro)"],
        ["Fai clic sulla striscia", "Vai a quella foto"],
      ],
    },
  ] as { title: string; keys: [string, string][] }[],
  navCameras: "Fotocamere e supporti",
  navLibraryFolders: "Cartelle della libreria",
  navDrives: "Unità",
  navAddFolder: "Aggiungi una cartella",
  add: "Aggiungi",
  browse: "Sfoglia…",
  addFolderPlaceholder: folderExample("es. ", "utente"),
  pickLibraryFolder: "Scegli la cartella da aggiungere alla libreria",
  showMore: (n: number) => (n === 1 ? "un altro" : `altri ${num(n)}`),
  collapse: "Mostra meno",
  photosCount: (n: number) => `${num(n)}`,
  memoriesTitle: (years: number) =>
    `Oggi, ${num(years)} ${one(years, "anno", "anni")} fa`,
  viewerFavorite: "Preferita (F)",
  viewerPick: "Contrassegna (P)",
  viewerUnpick: "Togli il contrassegno (U)",
  viewerPicked: "Foto contrassegnata",
  judgeFav: "Preferita",
  judgeUnfav: "Tolta dai preferiti",
  judgePick: "Contrassegnata",
  judgeUnflag: "Segno rimosso",
  viewerReject: "Rifiuta (X)",
  viewerRejected: "Rifiutata",
  favoriteMark: "Preferita",
  viewerMutedHint: "Audio disattivato: fai clic per attivarlo",
  rejectChip: (n: number) => `✕ ${num(n)}`,
  rejectChipTitle: "Rivedi le foto rifiutate",
  rejectGateTitle: (n: number) => `Sposta ${num(n)} foto nel Cestino`,
  rejectGateNote: "Puoi ripristinarle dal Cestino (e tornano anche nella libreria).",
  rejectGateRestore: "Conserva",
  rejectGateBack: "Indietro",
  rejectGateDiscard: "Chiudi senza eliminare",
  rejectGateConfirm: (n: number) => `Sposta ${num(n)} foto nel Cestino`,
  rejectGateTrashing: (done: number, total: number) =>
    `Spostamento… (${num(done)} / ${num(total)})`,
  updateFound: (v: string) => `È disponibile la versione ${v}`,
  updateOpenPage: "Apri la pagina di download",
  updateLater: "Più tardi",
  updateCheckNow: "Verifica aggiornamenti",
  updateChecking: "Verifica in corso…",
  updateUpToDate: "Hai la versione più recente",
  updateFailed: "Impossibile verificare",
  updateOnStart: "Verifica aggiornamenti all'avvio",
  updateOnStartNote:
    "Chiede a GitHub il nome della versione più recente (una volta al giorno). Non vengono inviati né foto né nomi di file. Se lo disattivi, da questo computer non esce nulla, tranne quando premi “Verifica aggiornamenti”.",
  viewerSlideshow: "Presentazione (Barra spaziatrice)",
  // 抽出（Issue #13）
  extractSave: (key: string) => `Salva questa immagine in un file (${key})`,
  extractCopy: (key: string) => `Copia questa immagine negli Appunti (${key})`,
  extractSaveTitle: "Salva immagine con nome",
  extractFilter: "Immagine",
  extractSaved: "Salvata",
  extractCopied: "Copiata",
  extractCopying: "Copia in corso…",
  extractFailed: "Impossibile estrarre questa immagine",
  extractSameFile: "Impossibile sovrascrivere il file originale",
  viewerExif: "Dati della foto (I)",
  viewerFullscreen: "Schermo intero (F11)",
  viewerClose: "Chiudi (Esc)",
  viewerPrev: "Precedente (←)",
  viewerNext: "Successiva (→)",
  viewerFitToScreen: "Adatta allo schermo (0)",
  viewerActualSize: "Dimensioni reali, 100% (1) — o doppio clic",
  actualSizeBadge: "1:1",
  // 動画（第9部）
  videoUnsupported: "Questo formato non può essere riprodotto nell'app",
  fileMissing: "Questo file non c'è (forse una chiavetta USB o una scheda SD è scollegata, oppure il file è stato spostato o eliminato)",
  fileUnreachable: "Impossibile aprire questo file (l'unità non è pronta o l'accesso non è consentito)",
  fileNotShown: "Impossibile visualizzare questo file (il file è danneggiato oppure pictkura non sa leggere questo formato)",
  fileNotDownloaded: "Questo file non è ancora stato scaricato (riaprilo quando sei online)",
  videoCloudOnly: "Questo video è nel cloud",
  videoCloudOnlyNote:
    "Riprodurlo qui avvia un download e non si vede nulla finché non termina. Se lo apri nell'app predefinita, puoi seguire l'avanzamento del download.",
  videoFailed: "Impossibile riprodurre questo video",
  videoOpenExternal: "Apri nell'app predefinita",
  videoCodecNote:
    "I video degli iPhone e di fotocamere simili usano HEVC (H.265). Per riprodurli serve un decodificatore del sistema; su Windows è un'estensione a pagamento disponibile in Microsoft Store (pochi euro).",
  videoCodecNoteMac:
    "macOS decodifica HEVC di serie, quindi probabilmente si tratta di un formato di registrazione che non supporta.",
  videoCodecNoteOther:
    "Forse sul tuo sistema non è installato un decodificatore per questo formato di registrazione.",
  videoCodecHelp: "Ottieni Estensioni video HEVC (a pagamento)",
  loading: "Caricamento…",
  exifTitle: "Dati della foto",
  exifCamera: "Fotocamera",
  exifLens: "Obiettivo",
  exifAperture: "Diaframma",
  exifShutter: "Tempo di posa",
  exifIso: "ISO",
  exifFocal: "Lunghezza focale",
  exifLocation: "Luogo",
  exifNone: "Nessun dato EXIF",
  paletteInput: "Data, fotocamera, parola chiave o comando…",
  paletteNoResults: "Nessun risultato",
  paletteGroupJumpDate: "Vai a una data",
  paletteGroupRecentDays: "Giorni recenti",
  paletteGroupCameras: "Filtra per fotocamera",
  paletteGroupSearch: "Cerca",
  paletteGroupActions: "Azioni",
  paletteSearchFor: (q: string) => `Cerca “${q}”`,
  paletteSearchHint: "Nome file, cartella, fotocamera",
  paletteSelect: "Seleziona",
  paletteRun: "Esegui",
  paletteCloseHint: "Chiudi",
  actionShowFavorites: "Mostra solo le preferite",
  actionShowPicked: "Mostra solo le contrassegnate",
  actionShowAll: "Mostra tutte le foto",
  actionCalendar: "Vista calendario",
  actionThumbnails: "Griglia delle foto",
  indexBuilding: "🔍 Creazione dell'indice di ricerca… ",
  bgScanning: "Aggiornamento della libreria…",
  bgThumbs: (n: number) => `Lettura delle foto (${num(n)} ${one(n, "rimanente", "rimanenti")})`,
  /* ヘッダーに出す短い形。全文（上の2つ）はマウスを乗せたときに出す（dev #64、2026-10-17 利用者の選択） */
  bgScanningShort: "Aggiornamento",
  bgThumbsShort: (n: number) => `ancora ${num(n)}`,
  cameraScanning: "📷 Lettura dei dati della fotocamera… ",
  indexIncompleteWarning:
    "⚠ L'indicizzazione della ricerca è stata interrotta — i risultati potrebbero essere incompleti (riprende al prossimo avvio)",
  indexProgressSuffix: "% — finché non termina, i risultati potrebbero essere incompleti",
  removeRoot: (path: string) => `Rimuovi ${path} dalla libreria`,
  rootMissingNotice: (name: string, count: number) =>
    `La cartella “${name}” non c'è${count > 0 ? ` (${num(count)} ${one(count, "foto al suo interno non può essere aperta", "foto al suo interno non possono essere aperte")})` : ""}. Se è su una chiavetta USB o una scheda SD, inseriscila e premi “Ripeti scansione”.`,
  rootsMissingNotice: (names: string, count: number) =>
    `Alcune cartelle non ci sono: ${names}${count > 0 ? ` (in tutto ${num(count)} ${one(count, "foto non può essere aperta", "foto non possono essere aperte")})` : ""}. Se sono su una chiavetta USB o una scheda SD, inseriscila e premi “Ripeti scansione”. Per smettere di usare una cartella, rimuovila dalla libreria con la ✕ nell'elenco a sinistra (i file delle foto non vengono eliminati).`,
  rootRemoveFromLibrary: "Rimuovi dalla libreria",
  rootRemoveKeepsFiles: "Rimuove la cartella solo dalla libreria. I file delle foto non vengono eliminati",
  rootRemoveConfirm: (name: string) =>
    `Rimuovere “${name}” dalla libreria?\nI file delle foto non vengono eliminati, ma i segni ★ e ⚑ sulle sue foto sì.`,
  rootTempConfirm: (path: string) =>
    `“${path}” si trova in una cartella temporanea.\nIl sistema o altre app possono eliminare senza avvisare i file al suo interno. Aggiungerla comunque alla libreria?`,
  rootTempConfirmOk: "Aggiungi comunque",
  destTempConfirm: (path: string) =>
    `“${path}” si trova in una cartella temporanea.\nIl sistema o altre app possono eliminare senza avvisare le foto importate qui. Dopo aver cancellato la scheda, quelle foto non esisteranno da nessun'altra parte. Usarla comunque come destinazione?`,
  destTempConfirmOk: "Usa come destinazione",
  destTempWarning:
    "⚠ La destinazione si trova in una cartella temporanea. Le foto importate potrebbero sparire senza preavviso.",
  rootTempTip: (path: string) =>
    `${path} — si trova in una cartella temporanea. Il sistema o altre app possono eliminare senza avvisare i file al suo interno`,
  rootMissingTempTip: (path: string) =>
    `${path} — non c'è. Si trovava in una cartella temporanea, quindi il sistema o un'altra app potrebbe averla eliminata`,
  rootMissingTip: (path: string) =>
    `${path} — non c'è. Se è su una chiavetta USB o una scheda SD, inseriscila e premi “Ripeti scansione”`,
  importFrom: (path: string) => `Importa da ${path}`,
  filterByCamera: (name: string) => `Mostra solo le foto scattate con ${name}`,
  jumpToYear: (year: number) => `Vai al ${year}`,
  importing: (done: number, total: number) => `Importazione… ${num(done)}/${num(total)}`,
  importDone: (copied: number, skipped: number) =>
    `Importazione completata: ${num(copied)} ${one(copied, "copiata", "copiate")}, ${num(skipped)} ${one(skipped, "saltata", "saltate")}`,
  importFailed: (n: number) => `, ${num(n)} non ${one(n, "riuscita", "riuscite")}`,
  importIncomplete: " ⚠ Alcune cartelle non sono state lette — non cancellare ancora la scheda",
  syncDone: (added: number, changed: number, removed: number) =>
    `${num(added)} ${one(added, "aggiunta", "aggiunte")}, ${num(changed)} ${one(changed, "modificata", "modificate")}, ${num(removed)} ${one(removed, "rimossa", "rimosse")}`,
  drivesReturnedScanned: (added: number, changed: number) =>
    `Cartelle dell'unità inserita rilette (${num(added)} ${one(added, "aggiunta", "aggiunte")}, ${num(changed)} ${one(changed, "modificata", "modificate")})`,
  pickSource: "Scegli la cartella da cui importare (USB / DCIM)",
  pickDestination: "Scegli la cartella di destinazione",
  wizardTitle: "Importa",
  wizardSources: "Origine",
  wizardOtherFolder: "Altra cartella…",
  wizardRefresh: "Rileva di nuovo le unità",
  wizardRemovable: "Rimovibile",
  wizardNoDrives: "Nessuna unità trovata",
  emptyTitle: "Ancora nessuna foto",
  emptyTitleFailed: "Impossibile mostrare l'elenco",
  emptyTitleChecking: "Verifica in corso",
  emptyTitleStartupFailed: "La sincronizzazione all'avvio non è terminata",
  emptyStartupFailed:
    "La sincronizzazione eseguita all'avvio non è terminata. Potrebbero esserci foto non ancora entrate nell'elenco. Premi “Ripeti scansione”; se non basta, riapri l'app.",
  emptyTitleMissing: "Alcune posizioni non ci sono",
  emptyTitleUnreadable: "Alcune posizioni non si possono aprire",
  emptyNoRoots:
    "Non è ancora impostata nessuna cartella della libreria. Importa da una scheda o scegli una cartella che contiene foto.",
  emptyMissing: (names: string) =>
    `Queste posizioni non ci sono: ${names}. Se sono su una chiavetta USB o una scheda SD, inseriscila e premi “Ripeti scansione”.`,
  emptyUnreadableMac: (names: string) =>
    `Impossibile aprire queste posizioni: ${names}. In Impostazioni di Sistema → Privacy e sicurezza, consenti a pictkura di accedere a quella cartella (Scrivania, Documenti, un disco esterno). Se è in rete, controlla che sia connessa e premi “Ripeti scansione”.`,
  emptyUnreadableWin: (names: string) =>
    `Impossibile aprire queste posizioni: ${names}. Controlla le autorizzazioni della cartella. Se è un'unità di rete, controlla che sia connessa e premi “Ripeti scansione”.`,
  emptyUnreadableOther: (names: string) =>
    `Impossibile aprire queste posizioni: ${names}. Controlla di avere il permesso di leggerle, poi premi “Ripeti scansione”.`,
  listSeparator: ", ",
  listLastSeparator: " ",
  /**
   * 受ける名詞は `posizioni` / `cartelle`（`nameList()`）で、どちらも女性。
   * 名詞を書かない文もある——`emptyAllExcluded`（除外で飛ばした物。ファイルもフォルダも在る）と
   * 絞り込みの見出し（`joinConditions`。条件は最大3つなので畳まれない）。名詞を書く独語（`Orte`）と
   * 違って伊語は呼び出しごとに名詞を変えられないので、**名詞を書く文が多い女性に寄せる**
   * （2026-11-03、ゲート2が2巡目で女性へ、3巡目で男性へと反対向きに動かしたので、ここで止めた）
   */
  andMore: (n: number) => (n === 1 ? "e un'altra" : `e altre ${num(n)}`),
  emptyRootIsPackage:
    "Una delle cartelle della libreria è essa stessa una libreria di Foto. pictkura di proposito non legge all'interno di questo tipo di libreria, quindi da lì non comparirà mai nessuna foto. Scegli una cartella che contiene foto o importa da una scheda.",
  emptyPhotoLibrary:
    "Non è stato trovato nulla oltre alla libreria di Foto. Per impostazione predefinita pictkura non legge all'interno di Foto: la maggior parte degli originali si trova in iCloud, non su questo Mac. Importa da una scheda o scegli una cartella che contiene foto.",
  emptyManagedLibrary:
    "Non è stato trovato nulla oltre alle librerie di app di gestione foto (Foto, iPhoto o Aperture). pictkura di proposito non legge al loro interno: ne indicizzerebbe il contenuto una volta, ma non vedrebbe mai le modifiche successive. Importa da una scheda o scegli una cartella che contiene foto.",
  emptyRootIsManagedLibrary:
    "Una delle cartelle della libreria è essa stessa la libreria di un'app di gestione foto (Foto, iPhoto o Aperture). pictkura di proposito non legge all'interno di questo tipo di libreria, quindi da lì non comparirà mai nessuna foto. Scegli una cartella che contiene foto o importa da una scheda.",
  emptyAllExcluded: (names: string) =>
    `Tutto ciò che è stato trovato viene saltato dai modelli di esclusione (per esempio ${names}). Puoi modificarli in pictkura.toml, nella cartella delle impostazioni.`,
  emptyNothingHere:
    "Non sono ancora state trovate foto che pictkura possa leggere. Importa da una scheda o scegli una cartella che contiene foto.",
  calendarChecking: "Verifica in corso…",
  emptyTitleStalled: "Alcune posizioni non rispondono",
  emptyStalled: (names: string) =>
    `Queste posizioni non rispondono: ${names}. Se una di esse è in rete, controlla che sia ancora connessa e premi “Ripeti scansione”. Se non tornerà più, rimuovila dalle cartelle della libreria e compariranno i risultati delle altre.`,
  emptyChecking:
    "Le cartelle sono ancora in fase di verifica. Se una di esse è in rete, controlla che sia connessa e premi “Ripeti scansione”.",
  emptyLoadFailed:
    "Impossibile caricare l'elenco. Premi “Ripeti scansione” o riapri l'app.",
  wizardPickFolderHint: "Scegli una cartella a sinistra per vederne le foto",
  wizardNoImages: "Nessuna foto in questa cartella",
  wizardUnreadable: "Impossibile leggere questa cartella (potrebbe essere stata rimossa)",
  wizardCounting: "Caricamento…",
  wizardSelectAll: "Seleziona tutto",
  wizardSelectNew: "Seleziona solo le nuove",
  wizardClearSelection: "Deseleziona tutto",
  wizardSelected: (n: number) => `${num(n)} ${one(n, "selezionata", "selezionate")}`,
  wizardImportedBadge: "✓",
  wizardImportedTitle: "Già importata (lo stesso file è nella cartella di destinazione)",
  wizardUnsureBadge: "?",
  wizardUnsureTitle:
    "Questo nome compare più di una volta sulla scheda. Se il file nella destinazione sia questa foto non si sa finché non se ne legge il contenuto: l'importazione lo legge e non crea una seconda copia di una foto già presente",
  wizardDestination: "Destinazione",
  wizardChangeDestination: "Cambia",
  wizardStructure: "Organizzazione",
  wizardImportButton: (n: number) => `Importa ${num(n)} foto`,
  wizardImportAll: "Importa tutta questa cartella (sottocartelle incluse)",
  wizardImportAllShort: "Cartella intera",
  wizardDeep: "Includi le sottocartelle",
  wizardDeepHint: "Esplora tutto il supporto, così non devi sapere dove si trovano le foto",
  wizardScanning: "Esplorazione del supporto…",
  wizardTruncated: (n: number) =>
    `Vengono mostrate solo le prime ${num(n)}. Usa “Cartella intera” per importare tutto`,
  wizardScanIncomplete: "⚠ Alcune cartelle non sono state lette (potrebbero mancare foto)",
  decoderHeifNotice: (n: number) =>
    `⚠ ${num(n)} foto HEIC/HEIF su questo computer non ${one(n, "ha", "hanno")} miniatura e non si ${one(n, "apre", "aprono")} nemmeno. Servono Estensioni immagini HEIF (gratuite) e, per decodificare i pixel, Estensioni video HEVC (a pagamento, pochi euro)`,
  decoderHeifNoticeMac: (n: number) =>
    `⚠ ${num(n)} foto HEIC/HEIF su questo computer non ${one(n, "ha", "hanno")} miniatura e non si ${one(n, "apre", "aprono")} nemmeno`,
  decoderHeifNoticeOther: (n: number) =>
    `⚠ ${num(n)} foto HEIC/HEIF su questo computer non ${one(n, "ha", "hanno")} miniatura e non si ${one(n, "apre", "aprono")} nemmeno. Forse sul tuo sistema non è installato un decodificatore per HEIC/HEVC`,
  decoderHeifHow: "Estensioni immagini HEIF (gratuite)",
  decoderHevcHow: "Estensioni video HEVC (a pagamento)",
  decoderNoticeDismiss: "Non mostrare più",
  wizardOfflineTitle:
    "Questo file è nel cloud (qui non c'è anteprima; importandolo verrà scaricato)",
  wizardHideImported: "Nascondi quelle già importate",
  wizardAllImported: "Niente di nuovo qui (tutto il contenuto di questa cartella è già importato)",
  wizardHiddenCount: (n: number) =>
    `${num(n)} già ${one(n, "importata è nascosta", "importate sono nascoste")}`,
  /** 動詞だけ単数へ。`circa` は不変化の副詞なので1でも残す（西語の `unos` とは違う） */
  wizardEtaSeconds: (n: number) => `${one(n, "manca", "mancano")} circa ${num(n)} s`,
  wizardEtaMinutes: (n: number) => `${one(n, "manca", "mancano")} circa ${num(n)} min`,
  wizardEtaCalculating: "stima del tempo rimanente…",
  wizardCapped: (n: number) => `${num(n)}+`,
  wizardMoreFiles: (n: number) =>
    `${num(n)} in più (scorri per ${one(n, "caricarla", "caricarle")})`,
  menuOpen: "Apri",
  menuOpenWith: (name: string) => `Apri con ${name}`,
  menuOpenWithOther: "Apri con un'altra app…",
  menuReveal: "Mostra nella cartella",
  // Windows 11 のエクスプローラーの同じ項目と同じ言葉（ADR B-4）
  menuMoreOptions: "Mostra altre opzioni",
  menuCopyImage: "Copia immagine negli Appunti",
  menuRotateRight: "Ruota a destra",
  menuRotateLeft: "Ruota a sinistra",
  menuRotateRestore: "Ripristina l'orientamento originale",
  menuPrint: "Stampa…",
  printTitle: "Stampa",
  printFitContain: "Adatta alla pagina",
  printFitContainHint: "La foto entra tutta. Se le sue proporzioni sono diverse da quelle del foglio, restano dei margini.",
  printFitCover: "Ritaglia per riempire",
  printFitCoverHint: "Nessun margine, ma le parti della foto che escono dal foglio vengono tagliate.",
  printFitStretch: "Riempi la pagina (ignora le proporzioni)",
  printFitStretchHint: "Nessun margine e nulla di tagliato, ma la foto viene allungata in verticale o in orizzontale.",
  printBorderless: "Per stampare senza bordi, scegli un formato carta senza bordi nella finestra di stampa successiva.",
  printGo: "Stampa…",
  printPreparing: "Preparazione della stampa…",
  printSheetTitle: (n: number) => `Stampa ${num(n)} foto`,
  printPerPage: "Foto per pagina",
  printCaptions: "Aggiungi nome file e data di scatto",
  printTooMany: (max: number) => `Con questo numero di foto per pagina puoi stampare al massimo ${num(max)} foto alla volta. Aumenta le foto per pagina o selezionane meno.`,
  printPreparingN: (done: number, total: number) => `Preparazione della stampa… ${num(done)}/${num(total)}`,
  printSkipped: (n: number) =>
    `${num(n)} foto non ${one(n, "si può stampare (video, per esempio) ed è stata esclusa", "si possono stampare (video, per esempio) e sono state escluse")}.`,
  errPrint: "Impossibile preparare la foto per la stampa.",
  errShellMenu: "Impossibile mostrare il menu di Esplora file.",
  menuDelete: "Elimina (sposta nel Cestino)",
  menuInfo: "Informazioni",
  fileInfoTitle: "File",
  fileInfoName: "Nome",
  fileInfoFolder: "Cartella",
  fileInfoSize: "Dimensioni",
  fileInfoTaken: "Data scatto",
  fileInfoTakenUnknown: "Sconosciuta",
  fileInfoModified: "Ultima modifica",
  fileInfoDimensions: "Dimensioni in pixel",
  fileInfoDuration: "Durata",
  fileInfoType: "Tipo",
  selectionTotal: (size: string) => `${size} in totale`,
  menuFavoriteOn: "Aggiungi ai preferiti",
  menuFavoriteOff: "Rimuovi dai preferiti",
  pickEditor: "Scegli un'app per la modifica",
  deleteConfirm: (n: number) =>
    n === 1
      ? "Spostare questa foto nel Cestino?"
      : `Spostare ${num(n)} foto nel Cestino?`,
  deleted: (n: number) => `${num(n)} ${one(n, "spostata", "spostate")} nel Cestino`,
  deletedSomeLeft: (n: number, left: number) =>
    `${num(n)} ${one(n, "spostata", "spostate")} nel Cestino (${num(left)} non ${one(left, "è stata trovata ed è rimasta al suo posto", "sono state trovate e sono rimaste al loro posto")})`,
  deletedMissing: (n: number, missing: number) =>
    `${num(n)} ${one(n, "spostata", "spostate")} nel Cestino (${num(missing)} non ${one(missing, "era più nella libreria e non è stata spostata", "erano più nella libreria e non sono state spostate")})`,
  // 複数選択と一括操作
  selectItem: "Seleziona",
  selectedCount: (n: number) => `${num(n)} ${one(n, "selezionata", "selezionate")}`,
  selectAll: "Seleziona tutto",
  clearSelection: "Deseleziona tutto (Esc)",
  selectDay: "Seleziona tutto questo giorno",
  bulkFavoriteOn: "Aggiungi ai preferiti",
  bulkFavoriteOff: "Rimuovi dai preferiti",
  bulkDelete: "Sposta nel Cestino",
  bulkCopy: "Copia in una cartella",
  bulkMove: "Sposta in una cartella",
  bulkViewer: "Guarda le selezionate",
  pickExportFolder: "Scegli la cartella in cui esportare",
  pickMoveFolder: "Scegli la cartella in cui spostare",
  moveConfirm: (n: number) =>
    n === 1
      ? "Spostare questa foto in una cartella che sceglierai ora?\nLascia la posizione attuale ed esce dalla libreria.\nI segni ★ e ⚑ non vengono mantenuti."
      : `Spostare ${num(n)} foto in una cartella che sceglierai ora?\nLasciano la posizione attuale ed escono dalla libreria.\nI segni ★ e ⚑ non vengono mantenuti.`,
  confirmCancel: "Annulla",
  deleteConfirmOk: "Sposta nel Cestino",
  moveConfirmOk: "Scegli la destinazione",
  rootRemoveConfirmOk: "Rimuovi",
  exporting: (done: number, total: number, name: string) =>
    `Esportazione… ${num(done)}/${num(total)} ${name}`,
  exportDone: (done: number, skipped: number, failed: number, missing: number) => {
    const parts: string[] = [];
    // 書き出すものが無かったときに「0枚を書き出しました」から始めない（pictkura-dev #33）
    if (done > 0 || skipped + failed + missing === 0) parts.push(`${num(done)} foto ${one(done, "esportata", "esportate")}`);
    if (skipped > 0) parts.push(`${num(skipped)} ${one(skipped, "era già presente", "erano già presenti")}`);
    if (failed > 0) parts.push(`${num(failed)} non ${one(failed, "riuscita", "riuscite")}`);
    if (missing > 0) parts.push(`${num(missing)} non ${one(missing, "era più nella libreria e non è stata esportata", "erano più nella libreria e non sono state esportate")}`);
    return parts.join(". ") + ".";
  },
  moving: (done: number, total: number, name: string) =>
    `Spostamento… ${num(done)}/${num(total)} ${name}`,
  moveDone: (moved: number, skipped: number, failed: number, leftBehind: number, missing: number) => {
    const parts: string[] = [];
    if (moved > 0 || skipped + failed + leftBehind + missing === 0) parts.push(`${num(moved)} foto ${one(moved, "spostata", "spostate")}`);
    if (skipped > 0)
      parts.push(
        `${num(skipped)} ${one(skipped, "era già nella destinazione ed è rimasta al suo posto", "erano già nella destinazione e sono rimaste al loro posto")}`,
      );
    if (failed > 0)
      parts.push(
        `${num(failed)} non ${one(failed, "si è potuta spostare ed è rimasta al suo posto", "si sono potute spostare e sono rimaste al loro posto")}`,
      );
    if (leftBehind > 0)
      parts.push(
        `${num(leftBehind)} ${one(leftBehind, "è stata copiata, ma non si è potuta rimuovere dalla posizione originale", "sono state copiate, ma non si sono potute rimuovere dalla posizione originale")}`,
      );
    if (missing > 0) parts.push(`${num(missing)} non ${one(missing, "era più nella libreria e non è stata spostata", "erano più nella libreria e non sono state spostate")}`);
    return parts.join(". ") + ".";
  },
  bulkPickOn: "Contrassegna",
  bulkPickOff: "Togli il contrassegno",
  bulkPickDone: (n: number) => `${num(n)} foto ${one(n, "contrassegnata", "contrassegnate")}`,
  bulkUnpickDone: (n: number) => `Contrassegno tolto a ${num(n)} foto`,
  bulkFavoriteDone: (n: number) => `${num(n)} foto ${one(n, "aggiunta", "aggiunte")} ai preferiti`,
  bulkUnfavoriteDone: (n: number) => `${num(n)} foto ${one(n, "rimossa", "rimosse")} dai preferiti`,
  settings: "Impostazioni",
  close: "Chiudi",
  settingsTitle: "Impostazioni",
  settingsImportStructure: "Struttura delle cartelle di importazione",
  settingsImportStructureNote:
    "Come vengono archiviate le foto importate in base alla data di scatto. Riguarda migliaia di file, e cambiarlo in seguito richiede un lavoro notevole. Quando il nome di una cartella contiene una data, l'anno viene prima in tutte le lingue, così l'ordine per nome è anche l'ordine cronologico.",
  settingsDestination: "Destinazione",
  settingsDestinationUnset: "(non impostata: la sceglierai alla prima importazione)",
  settingsFlatExample: "IMG_0001.JPG (nessuna sottocartella)",
  settingsCustomPattern: "Personalizzata",
  settingsCustomPatternNote:
    "{year} {month} {day} vengono sostituiti dalla data. Usa / per creare livelli. I caratteri non validi e gli spostamenti alla cartella superiore (..) vengono rimossi automaticamente.",
  settingsCustomPatternResult: "Cartella risultante",
  settingsGrid: "Griglia delle foto",
  settingsStackRawJpegToggle: "Raggruppa le coppie RAW+JPEG in una sola miniatura",
  settingsStackRawJpegNote:
    "Un RAW e un JPEG con lo stesso nome nella stessa cartella appaiono come una sola miniatura, quella del JPEG. Nella griglia, ★, ⚑, eliminazione e selezione valgono per entrambi. Quale file vedere nella vista grande si sceglie in “Quando guardi una foto in grande”.",
  stackRawChipTitle: (files: number) =>
    `RAW e JPEG raggruppati in una sola miniatura (${num(files)} file)`,
  settingsStackBurstsToggle: "Raggruppa le raffiche in una sola miniatura",
  settingsStackBurstsNote:
    "Le foto scattate in rapida successione con la stessa fotocamera appaiono come una sola miniatura. Vengono raggruppate solo le foto il cui orario di scatto è registrato con frazioni di secondo (alcune fotocamere non lo registrano). Nella griglia, ★ e ⚑ dati dal menu del clic destro valgono solo per la foto di copertina; eliminazione e selezione valgono per tutta la raffica. Nella vista grande le raffiche non sono raggruppate: si scorrono una foto alla volta.",
  settingsBurstGap: "Intervallo massimo in una raffica",
  settingsOutgoing: "In uscita",
  settingsGoogleToggle: "Metti le foto importate anche nella cartella In uscita",
  settingsGoogleNote:
    "A ogni importazione, le foto vengono messe anche nella cartella In uscita (senza occupare altro spazio su disco). Delle coppie RAW+JPEG entra solo il JPEG. Se usi Google Foto: aggiungi questa cartella al backup delle cartelle di Google Foto.",
  settingsGoogleFolder: "Cartella",
  settingsGoogleOpenFolder: "Apri cartella",
  settingsGoogleCopyPath: "Copia percorso",
  settingsGoogleCopied: "Copiato",
  settingsGoogleCopyFailed: "Impossibile copiare. Seleziona il percorso e copialo.",
  settingsGoogleFolderNote: "Si trova sulla stessa unità della destinazione di importazione. Scegli un posto al di fuori delle cartelle controllate da altri backup, come un NAS o Amazon Photos.",
  settingsGoogleFoldersNote:
    "Cartelle In uscita in cui pictkura ha messo foto (ne crea una per unità). Se usi Google Foto: aggiungi al backup delle cartelle tutte quelle che usi.",
  settingsGoogleVideo: "Metti anche i video",
  settingsGoogleRawOnly: "Scatti solo RAW",
  googleRawOnlyNone: "Non mettere",
  googleRawOnlyEmbedded: "Metti il JPEG incorporato",
  settingsGoogleRawOnlyNote: "Cosa fare con i RAW scattati senza JPEG. “JPEG incorporato” estrae l'immagine di anteprima che la fotocamera ha salvato nel RAW e la mette con la data di scatto. Il JPEG estratto è un file a parte, quindi occupa spazio su disco.",
  settingsGoogleOneDrive: "Metti anche le foto in OneDrive",
  settingsGoogleOneDriveNote: "Se lo fai, OneDrive non potrà più rendere quelle foto “Solo online”.",
  settingsGoogleAfterNote: "Quando sposti una foto nel Cestino in pictkura, viene tolta anche dalla cartella In uscita. Se usi Google Foto: in Google Foto rimane. Il caricamento lo fa la pagina web di Google Foto (photos.google.com), quindi procede solo mentre quella pagina è aperta nel browser.",
  pickGoogleFolder: "Scegli la cartella In uscita",
  importGoogle: (n: number) => ` · ${num(n)} ${one(n, "aggiunto", "aggiunti")} a In uscita`,
  importGoogleFailed: (n: number) => `Impossibile aggiungerne ${num(n)} a In uscita (vedi il registro)`,
  importGoogleError: (why: string) => `Non è stato aggiunto nulla a In uscita: ${why}`,
  bulkSendGoogle: "Aggiungi a In uscita",
  bulkRemoveGoogle: "Togli da In uscita",
  calendarSendMonth: "Aggiungi questo mese a In uscita",
  /** 年は数ではなく名前（2,010年にしない）——文字列で受ける */
  calendarSendYear: (year: string) => `Aggiungi tutto il ${year} a In uscita`,
  googleSendSummary: (photos: number, videos: number, rawOnly: number, size: string) => {
    const parts: string[] = [];
    if (photos > 0) parts.push(`${num(photos)} foto`);
    if (videos > 0) parts.push(`${num(videos)} video`);
    const items: string[] = [];
    if (parts.length > 0) items.push(`${parts.join(", ")}${size ? ` (${size})` : ""}`);
    if (rawOnly > 0) items.push(`${num(rawOnly)} JPEG ${one(rawOnly, "estratto", "estratti")} da scatti solo RAW`);
    return `Da mettere nella cartella In uscita: ${items.join("; ")}.`;
  },
  googleSendLeftOut: (n: number) =>
    `${num(n)} degli elementi scelti non ${one(n, "viene messo", "vengono messi")} (il RAW di una coppia RAW+JPEG, i file in OneDrive e simili).`,
  googleSendFolders: (list: string) => `Destinazione: ${list}`,
  googleSendNewFolder: (path: string) => `Verrà creata ${path}.`,
  googleSendGoogleNote:
    "Se usi Google Foto: il caricamento lo fa la pagina web di Google Foto (photos.google.com), quindi procede mentre quella pagina è aperta nel browser.",
  googleSendGoogleNoteNew:
    "Se usi Google Foto: aggiungi anche ogni nuova cartella al backup delle cartelle di Google Foto. Il caricamento lo fa la pagina web di Google Foto (photos.google.com), quindi procede mentre quella pagina è aperta nel browser.",
  googleFolderCreated: (path: string) =>
    `${path} è stata creata. Se usi Google Foto: aggiungi anche questa cartella al backup delle cartelle.`,
  googleSendNothing: "Non c'è nulla da mettere.",
  googleSendConfirmOk: "Metti",
  googleSent: (total: number, already: number) =>
    `${num(total)} ${one(total, "aggiunto", "aggiunti")} a In uscita${already > 0 ? ` (${num(already)} ${one(already, "c'era già", "c'erano già")})` : ""}`,
  googleSentCloudOnly: (n: number) =>
    `${num(n)} ${one(n, "è solo nel cloud e non è stato messo", "sono solo nel cloud e non sono stati messi")}. ${one(n, "Scaricalo", "Scaricali")} prima, poi riprova.`,
  googleRemoved: (n: number) => `${num(n)} ${one(n, "tolto", "tolti")} da In uscita`,
  googleRemoveFailed: (n: number) =>
    `${num(n)} non ${one(n, "si è potuto togliere", "si sono potuti togliere")} ora (forse l'unità non è collegata). pictkura riproverà al prossimo avvio.`,
  googleSentLeftOut: (n: number) =>
    `${num(n)} non ${one(n, "è stato messo", "sono stati messi")} (il RAW di una coppia RAW+JPEG, i file in OneDrive e simili).`,
  burstGapOption: (seconds: number) => `${num(seconds)} s`,
  burstChip: (frames: number) => `▤ Raffica ${num(frames)}`,
  burstChipShort: (frames: number) => `▤ ${num(frames)}`,
  spreadStack: "Espandi per vederle una per una",
  foldStack: "Comprimi",
  burstTitle: (frames: number, spanMs: number, files: number) =>
    `Raffica di ${num(frames)} ${one(frames, "scatto", "scatti")} in ${num(Math.round(spanMs / 100) / 10)} s (${num(files)} file)`,
  burstTitleRawJpeg: (frames: number, spanMs: number, files: number) =>
    `Raffica di ${num(frames)} ${one(frames, "scatto", "scatti")} in ${num(Math.round(spanMs / 100) / 10)} s, RAW+JPEG (${num(files)} file)`,
  settingsViewer: "Quando guardi una foto in grande",
  settingsAutoAdvanceToggle: "Passa alla foto successiva dopo P / U",
  settingsAutoAdvanceNote:
    "Nella vista grande, P mette il contrassegno ⚑ alla foto (un elenco separato dai ★ Preferiti) e U lo toglie. Con questa opzione attiva, la foto successiva arriva subito, così basta un tasto per foto. Se è disattivata, resti sulla stessa foto.",
  settingsPairView: "Cosa mostrare di una coppia RAW+JPEG",
  pairViewJpeg: "Solo il JPEG",
  pairViewRaw: "Solo il RAW",
  pairViewBoth: "Entrambi (prima il RAW, poi il JPEG)",
  settingsPairViewNote:
    "Come si scorre nella vista grande una coppia che nella griglia è una sola miniatura. Se ne mostri solo uno, ★, ⚑, ✕ ed eliminazione nella vista grande valgono per entrambi i file della coppia. Se li mostri entrambi, valgono solo per il file che stai guardando. Puoi cambiarlo anche con i pulsanti RAW / JPEG della vista grande. Se le coppie non sono raggruppate nella griglia, compaiono entrambi i file.",
  viewerPairSide: "Quale file della coppia vedere (rimane anche nelle impostazioni)",
  settingsTransition: "Passaggio alla foto successiva",
  settingsSlideshowTransition: "Transizione della presentazione",
  transitionNone: "Nessuna",
  transitionSlide: "Scorrimento",
  transitionFade: "Dissolvenza",
  settingsTransitionNote:
    "Come la vista grande passa alla foto successiva quando avanzi (tasti freccia, pulsanti, la striscia in basso o l'avanzamento automatico dopo aver contrassegnato o rifiutato una foto) e quando una presentazione avanza. Lo scorrimento segue la direzione in cui avanzi. Se il sistema è impostato per ridurre il movimento, le foto cambiano senza animazione, qualunque opzione tu scelga.",
  settingsAutoplay: "Quando inserisci un'unità USB o una scheda SD",
  settingsAutoplayNote:
    "Se hai installato pictkura con il programma di installazione o da Microsoft Store, “pictkura で写真を取り込む” (importa foto con pictkura) compare tra le opzioni quando inserisci un'unità; la versione portatile non la offre. Non si avvia finché non la scegli. Dopo averla scelta, Windows la ricorda come azione predefinita per quel tipo di supporto (schede di memoria e unità USB si impostano separatamente) e apre la finestra di importazione ogni volta che inserisci un supporto dello stesso tipo. L'azione predefinita si cambia nelle impostazioni di AutoPlay di Windows. La voce stessa è scritta in giapponese, tranne con l'MSI in inglese, dove si legge “Import photos with pictkura”.",
  settingsAutoplayOpenWindows: "Apri le impostazioni di AutoPlay di Windows",
  settingsImportWaitingFrom: (path: string) =>
    `Un'importazione da ${path} è in attesa. Si aprirà quando chiudi le impostazioni.`,
  settingsImportWaiting: "Un'importazione è in attesa. Si aprirà quando chiudi le impostazioni.",
  settingsImportWaitingOpen: "Chiudi e importa",
  settingsAbout: "Informazioni",
  settingsAboutLicense: "Distribuito con licenza MIT.",
  settingsManual: "Manuale",
  settingsOssLicenses: "Software open source che usiamo",
  settingsDocNotBundled: "(non incluso in una build di sviluppo)",
  settingsLog: "Apri il registro",
  settingsLogNone: "(ancora nessuna voce registrata)",
  settingsOpenFailed: "Impossibile aprire.",
  settingsLogNote:
    "Viene scritta una riga solo quando qualcosa non riesce, e rimane su questo computer. Non viene inviato nulla.",
  settingsLanguage: "Lingua",
  settingsLanguageSystem: "Come il sistema",
  settingsLanguageNote:
    "Cambiandola, la finestra si ricarica. La copia continua in background, ma l'importazione guidata si chiude e non ne vedi più l'avanzamento: aspetta che un'importazione finisca prima di cambiare lingua.",
  settingsLanguageMacNote:
    "Le finestre di macOS, come il pannello di salvataggio e le finestre di conferma, passeranno a questa lingua dal prossimo avvio di pictkura.",
  settingsTheme: "Tema",
  themeSystem: "Come il sistema",
  themeLight: "Chiaro",
  themeDark: "Scuro",
  settingsEditors: "App per la modifica",
  settingsEditorsNote: "App che hai scelto con “Apri con un'altra app…”.",
  settingsForgetEditor: "Rimuovi dall'elenco",
  calendarEmpty: "Nessuna foto",
  filterEmptyTitle: (conds: string) => `Nessuna foto corrisponde a questi filtri: ${conds}`,
  filterEmptyMessage: "Togli la ricerca o i filtri scelti a sinistra per vedere le altre foto.",
  filterEmptyStillIndexing:
    "Le foto sono ancora in fase di importazione o indicizzazione, quindi potrebbero comparirne altre.",
  filterCondQuery: (q: string) => `la ricerca “${q}”`,
  filterCondCamera: (name: string) => `scattate con ${name}`,
  filterEmptyClearSearch: "Cancella la ricerca",
  errGoogleOverlapsRoot: "Non puoi scegliere una cartella della libreria, una cartella al suo interno né una che la contiene.",
  errGoogleInsideSync: "Non puoi scegliere un posto all'interno di una cartella sincronizzata (OneDrive, Google Drive, Dropbox, iCloud Drive e simili).",
  errGoogleNoHardLinks: "Questa unità non va bene (exFAT, FAT e simili non possono creare collegamenti fisici).",
  errGoogleWholeVolume: "La destinazione di importazione è un'intera unità, quindi non c'è posto per la cartella In uscita. Cambia la destinazione di importazione.",
  errGoogleNoName: "Non puoi scegliere l'unità stessa. Scegli una cartella al suo interno.",
  errGoogleLinkInTheWay: "La cartella In uscita è un collegamento simbolico. Scegli un altro posto.",
  errGoogleIo: "Impossibile leggere o scrivere nella cartella In uscita.",
  errGoogleOtherDrive: "Scegli una cartella sulla stessa unità della destinazione di importazione.",
  errNotFound: "Quella foto non è più nell'indice.",
  errRotateNoOrientation: "Questa foto non ha informazioni sull'orientamento, quindi non può essere ruotata.",
  errRotateXmp: "Sarebbe in conflitto con l'orientamento che i programmi di fotoritocco leggono dall'XMP, quindi non può essere ruotata.",
  errRotateCloudOnly: "Questa foto è solo nel cloud, quindi non può essere ruotata. Scaricala e riprova.",
  errRotateCameraCard: "Le foto su una scheda della fotocamera vengono lasciate come sono, perché si importano da lì. Importala, poi ruotala.",
  errRotateNotReady: "Lettura della foto ancora in corso. Attendi un momento e riprova.",
  errRotateChanged: "Il file è stato modificato altrove durante la rotazione. Riprova.",
  errRotateBusy: "Verifica delle cartelle in corso. Attendi un momento e riprova.",
  errRotateWrite: "Impossibile scrivere nel file, quindi non è stato ruotato. Controlla che nessun'altra app lo tenga aperto e che non sia di sola lettura.",
  errRotateLeftChanged: "Qualcosa non è andato a buon fine dopo la riscrittura del file e non è stato possibile ripristinarlo. La foto potrebbe essere rimasta ruotata. Per i dettagli, consulta il registro.",
  errRotateCannotRestore: "Questa foto non può più tornare all'orientamento che aveva prima che pictkura la ruotasse, per esempio perché un'altra app l'ha modificata.",
  errDb: "Impossibile leggere o scrivere l'indice.",
  errConfigIo: "Impossibile leggere o scrivere il file delle impostazioni.",
  errConfigFormat: "Impossibile interpretare il file delle impostazioni.",
  errNoDestination: "Non è ancora stata scelta una cartella di destinazione.",
  errRootManaged:
    "Una libreria gestita da un'app non può essere aggiunta come cartella: il contenuto appartiene a quell'app.",
  errDestManaged:
    "Una libreria gestita da un'app non può essere la destinazione: il contenuto appartiene a quell'app.",
  errNoImageToExtract: "In questo file non c'è nessuna immagine da estrarre.",
  errSourceUnreadable: "Impossibile leggere quella cartella.",
  errSourceManaged:
    "Non puoi importare dall'interno di una libreria gestita da un'app. Togli l'estensione (.photoslibrary e simili) dal nome della cartella e sceglila di nuovo.",
  errExportDest: "Impossibile creare quella cartella di destinazione.",
  errExportManaged: "Lì non si può scrivere: quel pacchetto è gestito da un'altra app.",
  errFolderMissing: "Quella cartella non c'è.",
  fileOpBusy: "Attendi che l'operazione in corso finisca, poi riprova.",
  errFolderNotAbsolute: "Inserisci il percorso completo della cartella.",
  errTrashFailed: "Impossibile spostare nel Cestino.",
  errTrashUnreachable: "L'unità o la cartella non è raggiungibile, quindi non è stato spostato nulla nel Cestino.",
  errTrashPartly: (n: number) =>
    `${num(n)} ${one(n, "spostata", "spostate")} nel Cestino; le altre non si sono potute spostare.`,
  errNoLogYet: "Ancora nessuna voce registrata.",
  errOutgoingFolderMissing: "La cartella In uscita non c'è. Viene creata quando attivi In uscita (se è già attiva, disattivala e riattivala).",
  errOpenOutgoingFailed: "Impossibile aprire la cartella In uscita. Usa “Copia percorso” e aprila da lì.",
  errNotBundled: "Non è incluso in questa build.",
  errNoStoreLink: "Per questo sistema non c'è una pagina da indicare.",
  errBadKind: "Impossibile stabilire come aprire questo elemento.",
  errTooManyIds: (n: number) => `Al massimo ${num(n)} alla volta.`,
};
