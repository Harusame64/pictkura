/**
 * 中南米のスペイン語（`es-419`）。**`es.ts`（本国・`es-ES`）との差分だけを持つ。**
 *
 * ## なぜ全文を書き写さないのか
 *
 * **2つのスペイン語が食い違うのは、この辞書では17キーだけ**である。残りは1文字も違わない。
 * 全文を写すと、**同じ文が2か所**になり、片方を直したときにもう片方が黙って古びる
 * ——**多言語の直しが伝播しないのは、この形から起きる**。差分だけ持てば、
 * `es.ts` の文言を直したときに**共通の文はここにも自動で効く**。
 *
 * **引き換えに失うもの**: 「250キー全部を中南米の目で見た」という証拠が残らない。
 * だから**探し方のほうを残す**——下の `AMERICAS_SWAPS` が、
 * **どの語で割れるかを名指しした一覧**であり、`i18n.test.ts` が
 * **その語が1つも残っていないこと**を機械で見る。
 *
 * ## どこで割れるか（実測。**推測で足さない**）
 *
 * | 本国 | 中南米 | 出どころ |
 * |---|---|---|
 * | `Vídeo(s)` | `Video(s)` | macOS 自身（`Localizable.loctable` の `es` / `es_419`） |
 * | `Ajustes del Sistema` | `Configuración del Sistema` | 同上 |
 * | `Añadir` | `Agregar` | 一般語。Adobe/Microsoft とも中南米版は `Agregar` |
 * | `descodificar` 系 | `decodificar` 系 | 一般語 |
 * | `Extensiones de vídeo HEVC` | `Extensiones de video HEVC` | **Microsoft Store 自身の製品名** |
 * | `unos pocos euros` | `unos pocos dólares` | 通貨。日英独が `数百円` / `a few dollars` / `ein paar Euro` と土地に合わせているのと同じ |
 * | `Ratón` | `Mouse` | 一般語。macOS も中南米では `mouse` |
 *
 * ## 意図して**変えていない**もの
 *
 * - **`papelera`。** macOS は中南米で `Basurero` と言う（`Common.loctable` で確認）が、
 *   **Windows は両方の地域で `Papelera de reciclaje`**。中国語のゴミ箱と同じ理由で
 *   **Windows 側へ寄せる**（`zh.ts` の冒頭に同じ判断がある）。分けるなら1キーではなく、
 *   OSで出し分ける仕掛けごと要る
 * - **`pulsar` 系**（`pulsa` / `púlsala` / `pulsaste`）。中南米では
 *   `hacer clic` / `presionar` のほうが自然だが、**通じない語ではない**し、
 *   直すと文ごと組み替えることになる。**語の置き換えではなく書き直し**なので、
 *   ここでは踏み込まない。踏み込むならその判断ごと別に立てる
 *
 * ## 言語を足したのではなく、地域を足した
 *
 * `pickLocale()` は **`es-MX` のような地域つきのタグをここへ寄せる**（包含解決）。
 * `es-ES` と、地域が分からないときは `es.ts` のまま。規則はあちらにある。
 */
import { es } from "./es.ts";
import { num, one } from "./plural.ts";
import type { Dict } from "./ja.ts";

/**
 * **2つのスペイン語が割れる語**（左が本国、右が中南米）。**この一覧が規則である。**
 *
 * `i18n.test.ts` が3方向から使う:
 *
 * 1. **左の語が `es-419` に1つも残っていないこと**——差分の書き忘れがここで落ちる
 * 2. **左の語が `es.ts` にはまだ在ること**（語ごとに見る）——本国側の文言を変えて
 *    この一覧が古びたら落ちる。**古びた一覧は、検査していないのに検査したように見える**
 * 3. **ショートカット一覧が、この置き換えを当てただけの写しであること**
 *    ——あそこは入れ子なので丸ごと持つしかなく、**放っておくと本国側だけ育つ**（ゲート2）
 *
 * **1と2は大小を無視して見る**（`Vídeos` も `vídeo` で拾う）。**3は大小のまま当てる**
 * ——置き換える相手は文そのものなので、`Ratón` を `mouse` にしてしまっては困る。
 *
 * **2を語ごとに見るようにした途端、`ratón` が「本国側に無い規則」として落ちた**
 * ——大文字の `Ratón` しか使っていなかった。**死んだ規則は、検査したふりをする。**
 */
export const AMERICAS_SWAPS: readonly (readonly [string, string])[] = [
  ["vídeo", "video"],
  ["Añadir", "Agregar"],
  ["descodific", "decodific"],
  ["Ajustes del Sistema", "Configuración del Sistema"],
  ["euros", "dólares"],
  ["Ratón", "Mouse"],
];

export const es419: Dict = {
  ...es,

  // Vídeo → Video（macOS 自身の綴り）
  kindVideo: "Videos",
  videoCloudOnly: "Este video está en la nube",
  videoFailed: "No se ha podido reproducir este video",
  videoCodecNote:
    "Los videos de los iPhone y de cámaras parecidas usan HEVC (H.265). Para reproducirlos, el sistema necesita un decodificador; en Windows es una extensión de pago de Microsoft Store (unos pocos dólares).",
  videoCodecNoteMac:
    "macOS decodifica HEVC de serie, así que lo más probable es que sea un formato de grabación que no sabe manejar.",
  videoCodecNoteOther:
    "Puede que a tu sistema le falte un decodificador para este formato de grabación.",
  videoCodecHelp: "Conseguir las Extensiones de video HEVC (de pago)",
  decoderHevcHow: "Extensiones de video HEVC (de pago)",

  // Añadir → Agregar
  navAddFolder: "Agregar una carpeta",
  add: "Agregar",
  pickLibraryFolder: "Elige la carpeta que quieres agregar a la biblioteca",
  menuFavoriteOn: "Agregar a Favoritos",
  bulkFavoriteOn: "Agregar a Favoritos",

  // **macOSの設定の名前が地域で違う**（`Ajustes` / `Configuración`）
  emptyUnreadableMac: (names: string) =>
    `Estos lugares no se han podido abrir: ${names}. Dale a pictkura acceso a esa carpeta (Escritorio, Documentos, un disco externo) en Configuración del Sistema → Privacidad y seguridad. Si está en una red, comprueba que sigue conectada y pulsa «Volver a explorar».`,

  // 製品名・通貨・`decodificar`
  decoderHeifNotice: (n: number) =>
    `⚠ ${num(n)} ${one(n, "foto", "fotos")} HEIC/HEIF no ${one(n, "tiene", "tienen")} miniatura aquí, y tampoco se ${one(n, "puede", "pueden")} abrir. Hacen falta las Extensiones de imagen HEIF (gratis) y, además, las Extensiones de video HEVC (de pago, unos pocos dólares), que son las que decodifican los píxeles`,
  decoderHeifNoticeOther: (n: number) =>
    `⚠ ${num(n)} ${one(n, "foto", "fotos")} HEIC/HEIF no ${one(n, "tiene", "tienen")} miniatura aquí, y tampoco se ${one(n, "puede", "pueden")} abrir. Puede que a tu sistema le falte un decodificador para HEIC/HEVC`,

  // **ショートカット一覧は入れ子なので、丸ごと差し替えるしかない。**
  // 変えたのは `Ratón` → `Mouse`・`Añadir` → `Agregar`・`vídeo` → `video` の3語だけで、
  // ほかは `es.ts` と1文字も違わない。**`i18n.test.ts` が上の置き換えを当てて突き合わせる**
  // ので、本国側に行を足してここを忘れると落ちる（ゲート2）
  shortcutGroups: [
    {
      title: "Cuadrícula",
      keys: [
        ["Ctrl+K / ⌘K", "Paleta de comandos (ir a una fecha o a una cámara, buscar, importar)"],
        ["Ctrl+A / ⌘A", "Seleccionar todo lo que coincide con la búsqueda y el filtro actuales"],
        ["Mayús + clic", "Seleccionar todo lo que hay entre la última foto que pulsaste y esta"],
        ["Ctrl + clic", "Agregar o quitar una foto (⌘ + clic en macOS)"],
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
        ["U", "Quitar la marca de esta foto (⚑ y ✕)"],
        ["Ctrl+C / ⌘C", "Copiar al portapapeles la imagen que hay en pantalla"],
        ["Ctrl+S / ⌘S", "Guardar en un archivo la imagen que hay en pantalla"],
        ["F", "Poner o quitar favorita (★)"],
        ["I", "Datos de la toma (cámara, objetivo, diafragma, ISO, GPS)"],
        ["Espacio", "Pase de diapositivas. En un video, reproducir / pausar"],
        ["1 / 0", "Tamaño real 100 % / ajustar a la ventana"],
        ["F11", "Pantalla completa"],
        ["Esc", "Cerrar"],
      ],
    },
    {
      title: "Mouse (en la vista grande)",
      keys: [
        ["Doble clic", "Tamaño real 100 % ⇔ ajustar a la ventana"],
        ["Rueda", "Acercar / alejar"],
        ["Arrastrar", "Moverse por la imagen ampliada"],
        ["Clic derecho", "Abrir / abrir con / mostrar en la carpeta / mover a la papelera"],
        ["Pulsar la tira", "Ir a esa foto"],
      ],
    },
  ] as { title: string; keys: [string, string][] }[],
};
