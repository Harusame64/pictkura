/**
 * ポルトガル語辞書。キーの正は `ja.ts`——抜けや余りがあればコンパイルエラーになる。
 *
 * **ブラジルのポルトガル語（pt-BR）で書く。ポルトガル（pt-PT）の人にもこれを出す**
 * （2026-11-04 の利用者の判断）。話者の大半がブラジルで、語彙と呼びかけ
 * （`você` / `tu`）・綴り（`arquivo` / `ficheiro`、`tela` / `ecrã`）が割れるので、
 * 片方に決めないと書けない。本国版が要るようになったら `es-419` と同じく
 * 差分の辞書を別に足せばよい（この辞書を触らずに済む）。
 *
 * **語は発明せず、既にブラジルで使われているものへ合わせる**（独・西・仏と同じ方針）:
 *
 * - **⚑ / ✕ / U は Lightroom Classic の pt-BR 版に合わせる**——`escolhida` /
 *   `rejeitada` / `sem sinalizador`（P / X / U）。⚑ と ✕ の両方を覆う語は
 *   `sinalizador`（Lightroom でも ✕ は「却下の sinalizador」）。`judgeUnflag` と
 *   `U` の行は ✕ を外したときにも出るので、⚑ 限定の `escolhida` を使わず
 *   `sinalizador` にしてある
 * - **⚑ に `seleção` / `selecionar` を使わない**。複数選択がそちらなので、
 *   西語の `indicador`・仏語の `retenir` と同じ理由で ⚑ 側は `escolher` 系に限る
 * - **アプリの蔵書は `biblioteca`、写真アプリの蔵書は `Fototeca`**（macOS の
 *   pt-BR 版自身の語。西語の `biblioteca` / `fototeca` と同じ分け方）
 * - **OSの用語は引いてくる**。Apple の pt_BR（loctable で確認）: `Cancelar`・
 *   `Apagar`・`Girar à Esquerda` / `Girar à Direita`・`Imprimir…`・`Salvar`・
 *   `Selecionar Tudo`・`Copiar`、macOS の `Ajustes do Sistema → Privacidade e
 *   Segurança`。Windows の pt-BR: `Reprodução Automática`・`Mostrar mais opções`
 *   （Windows 11 のエクスプローラー）・`Explorador de Arquivos`・`links físicos`
 *   （ハードリンク）・Microsoft Store の `Extensões de Imagem HEIF` /
 *   `Extensões de Vídeo HEVC`。キーは `Ctrl`・`Shift`・`Espaço`
 * - **ゴミ箱は一般名詞の `lixeira`**（英語の `trash` と同じく小文字の普通名詞）。
 *   Windows の `Lixeira`、macOS の `Lixo` のどちらで読んでも通じる語にする
 * - **アプリの設定は `Configurações`**（ブラジルのソフトウェアで最も普通の語。
 *   Windows の設定アプリも `Configurações`）
 * - **金額はレアルに直す**（`alguns reais`。独西仏がユーロに直すのと同じ扱いで、直訳しない）
 * - **呼びかけは você**（ブラジルのソフトウェアUIの標準）。釦は不定詞
 *   （`Apagar` / `Cancelar` のような Apple・Microsoft の pt-BR の書き方）、見出しは名詞
 * - 引用符は “ ”、省略は …
 *
 * **単複は `one()`（1 だけ単数）。** CLDR の `pt` は 0 も単数に数えるが、
 * ブラジルの画面の文では `0 fotos` が普通で、`0 foto` は不自然に読める。
 * 仏語（`0 photo` が正しい）とはここが違うので、仏語の `un()` は持ち込まない。
 * **性の一致**: `foto`・`pasta`・`biblioteca` は女性（`1 foto escolhida`）、
 * `vídeo`・`arquivo`・`item` は男性。
 */
import { folderExample } from "./folderExample.ts";
import { num, one } from "./plural.ts";
import type { Dict } from "./ja.ts";

export const pt: Dict = {
  appName: "pictkura",
  viewThumbnails: "Fotos",
  viewCalendar: "Calendário",
  searchPlaceholder: "Buscar arquivos, pastas, câmeras, 2019-08 ou year:2019",
  searchClear: "Limpar busca (Esc)",
  commandPalette: "Paleta de comandos",
  importFromUsb: "Importar de USB",
  rescan: "Atualizar",
  size: "Tamanho",
  itemsCount: (n: number) => `${num(n)} ${one(n, "item", "itens")}`,
  navPlaces: "Locais",
  navAllPhotos: "Todas as fotos",
  navFavorites: "★ Favoritas",
  navPicked: "⚑ Escolhidas",
  navOutgoing: "Saída",
  outgoingGuide: "Escolha esta pasta de Saída como pasta para fazer backup (sincronizar) no app do seu serviço de fotos. A partir daí, cada importação coloca as fotos aqui e o serviço as recolhe. Se você usa o Google Fotos: adicione esta pasta ao backup de pastas do Google Fotos.",
  navKinds: "Tipo",
  kindPhoto: "Fotos",
  kindRaw: "RAW",
  kindVideo: "Vídeos",
  shortcutsTitle: "Atalhos de teclado (?)",
  keyCtrl: "Ctrl",
  actionShortcuts: "Mostrar atalhos de teclado",
  shortcutGroups: [
    {
      title: "Grade",
      keys: [
        ["Ctrl+K / ⌘K", "Paleta de comandos (ir para uma data ou câmera, buscar, importar)"],
        ["Ctrl+, / ⌘,", "Abrir as Configurações (o mesmo painel da engrenagem na barra)"],
        ["Ctrl+A / ⌘A", "Selecionar tudo o que a busca e o filtro atuais mostram"],
        ["Shift + clique", "Selecionar tudo entre a última foto clicada e esta"],
        ["Ctrl + clique", "Adicionar ou remover uma foto (⌘ + clique no macOS)"],
        ["Clicar no título de uma data", "Selecionar o dia inteiro (clique de novo para limpar)"],
        ["Esc", "Parar de selecionar"],
      ],
    },
    {
      title: "Visualizador",
      keys: [
        ["← / →", "Foto anterior / próxima"],
        ["P", "Escolher (⚑). Por padrão, passa para a próxima foto"],
        ["X", "Rejeitar (✕). As fotos rejeitadas vão para a lixeira quando você fecha o visualizador"],
        ["U", "Desfazer o sinalizador desta foto (limpa ⚑ e ✕)"],
        ["Ctrl+C / ⌘C", "Copiar a imagem da tela para a área de transferência"],
        ["Ctrl+S / ⌘S", "Salvar a imagem da tela em um arquivo"],
        ["Ctrl+P / ⌘P", "Imprimir a foto da tela"],
        ["F", "Marcar / desmarcar como favorita (★)"],
        ["I", "Detalhes do arquivo e da captura (câmera, lente, abertura, obturador, ISO, distância focal, GPS)"],
        ["Espaço", "Apresentação de slides. Em um vídeo, reproduzir / pausar"],
        ["1 / 0", "1 alterna entre tamanho real 100% ⇔ ajustar à tela; 0 sempre ajusta"],
        ["F11", "Tela cheia"],
        ["Esc", "Fechar"],
      ],
    },
    {
      title: "Mouse (no visualizador)",
      keys: [
        ["Clique duplo", "Tamanho real 100% ⇔ ajustar à tela"],
        ["Roda", "Ampliar / reduzir"],
        ["Arrastar", "Mover a imagem ampliada"],
        ["Clique direito", "Menu (abrir, copiar, girar, imprimir, informações, apagar e mais)"],
        ["Clicar na faixa", "Ir para essa foto"],
      ],
    },
  ] as { title: string; keys: [string, string][] }[],
  navCameras: "Câmeras e mídias",
  navLibraryFolders: "Pastas da biblioteca",
  navDrives: "Unidades",
  navAddFolder: "Adicionar pasta",
  add: "Adicionar",
  browse: "Procurar…",
  addFolderPlaceholder: folderExample("ex.: ", "usuario"),
  pickLibraryFolder: "Escolha uma pasta para adicionar à biblioteca",
  showMore: (n: number) => `mais ${num(n)}`,
  collapse: "Mostrar menos",
  photosCount: (n: number) => `${num(n)}`,
  memoriesTitle: (years: number) =>
    years === 1 ? "Há 1 ano, neste dia" : `Há ${num(years)} anos, neste dia`,
  viewerFavorite: "Favorita (F)",
  viewerPick: "Escolher (P)",
  viewerUnpick: "Desfazer escolha (U)",
  viewerPicked: "Foto escolhida",
  judgeFav: "Favorita",
  judgeUnfav: "Favorita removida",
  judgePick: "Escolhida",
  judgeUnflag: "Sinalizador removido",
  viewerReject: "Rejeitar (X)",
  viewerRejected: "Rejeitada",
  favoriteMark: "Favorita",
  viewerMutedHint: "Sem som — clique para ativar o som",
  rejectChip: (n: number) => `✕ ${num(n)}`,
  rejectChipTitle: "Revisar as fotos rejeitadas",
  rejectGateTitle: (n: number) =>
    n === 1 ? "Mover 1 foto para a lixeira" : `Mover ${num(n)} fotos para a lixeira`,
  rejectGateNote: "Você pode restaurá-las da lixeira (elas também voltam para a biblioteca).",
  rejectGateRestore: "Manter",
  rejectGateBack: "Voltar",
  rejectGateDiscard: "Fechar sem apagar",
  rejectGateConfirm: (n: number) => `Mover ${num(n)} para a lixeira`,
  rejectGateTrashing: (done: number, total: number) =>
    `Movendo… (${num(done)} / ${num(total)})`,
  updateFound: (v: string) => `A versão ${v} está disponível`,
  updateOpenPage: "Abrir a página de download",
  updateLater: "Mais tarde",
  updateCheckNow: "Verificar atualizações",
  updateChecking: "Verificando…",
  updateUpToDate: "Você está com a versão mais recente",
  updateFailed: "Não foi possível verificar",
  updateOnStart: "Verificar atualizações ao iniciar",
  updateOnStartNote:
    "Pergunta ao GitHub o nome da versão mais recente (uma vez por dia). Nenhuma foto ou nome de arquivo é enviado. Desligado, nada sai desta máquina, exceto quando você pressiona “Verificar atualizações”.",
  viewerSlideshow: "Apresentação de slides (Espaço)",
  extractSave: (key: string) => `Salvar esta imagem em um arquivo (${key})`,
  extractCopy: (key: string) => `Copiar esta imagem para a área de transferência (${key})`,
  extractSaveTitle: "Salvar imagem como",
  extractFilter: "Imagem",
  extractSaved: "Salva",
  extractCopied: "Copiada",
  extractCopying: "Copiando…",
  extractFailed: "Não foi possível extrair esta imagem",
  extractSameFile: "Não é possível salvar por cima do arquivo original",
  viewerExif: "Informações da foto (I)",
  viewerFullscreen: "Tela cheia (F11)",
  viewerClose: "Fechar (Esc)",
  viewerPrev: "Anterior (←)",
  viewerNext: "Próxima (→)",
  viewerFitToScreen: "Ajustar à tela (0)",
  viewerActualSize: "Tamanho real, 100% (1) — ou clique duplo",
  actualSizeBadge: "1:1",
  videoUnsupported: "Este formato não pode ser reproduzido no app",
  fileMissing: "Este arquivo não está lá (um pendrive ou cartão SD pode ter sido removido, ou o arquivo foi movido ou apagado)",
  fileUnreachable: "Este arquivo não pode ser aberto (a unidade não está pronta ou o acesso não é permitido)",
  fileNotShown: "Este arquivo não pode ser exibido (o arquivo está danificado ou o pictkura não lê este formato)",
  fileNotDownloaded: "Este arquivo ainda não foi baixado (abra-o de novo quando estiver on-line)",
  videoCloudOnly: "Este vídeo está na nuvem",
  videoCloudOnlyNote:
    "Reproduzi-lo aqui inicia um download e não mostra nada até terminar. Abri-lo no app padrão permite acompanhar o progresso do download.",
  videoFailed: "Não foi possível reproduzir este vídeo",
  videoOpenExternal: "Abrir no app padrão",
  videoCodecNote:
    "Os vídeos de iPhones e câmeras parecidas usam HEVC (H.265). A reprodução precisa de um decodificador do sistema; no Windows, é uma extensão paga da Microsoft Store (alguns reais).",
  videoCodecNoteMac:
    "O macOS decodifica HEVC de fábrica, então é mais provável que seja um formato de gravação que ele não suporta.",
  videoCodecNoteOther:
    "Seu sistema pode não ter um decodificador para este formato de gravação.",
  videoCodecHelp: "Obter as Extensões de Vídeo HEVC (pagas)",
  loading: "Carregando…",
  exifTitle: "Informações da foto",
  exifCamera: "Câmera",
  exifLens: "Lente",
  exifAperture: "Abertura",
  exifShutter: "Obturador",
  exifIso: "ISO",
  exifFocal: "Distância focal",
  exifLocation: "Local",
  exifNone: "Sem dados EXIF",
  paletteInput: "Data, câmera, palavra-chave ou comando…",
  paletteNoResults: "Nenhum resultado",
  paletteGroupJumpDate: "Ir para a data",
  paletteGroupRecentDays: "Dias recentes",
  paletteGroupCameras: "Filtrar por câmera",
  paletteGroupSearch: "Buscar",
  paletteGroupActions: "Ações",
  paletteSearchFor: (q: string) => `Buscar “${q}”`,
  paletteSearchHint: "Nome do arquivo, pasta, câmera",
  paletteSelect: "Selecionar",
  paletteRun: "Executar",
  paletteCloseHint: "Fechar",
  actionShowFavorites: "Mostrar só as favoritas",
  actionShowPicked: "Mostrar só as escolhidas",
  actionShowAll: "Mostrar todas as fotos",
  actionCalendar: "Visualização de calendário",
  actionThumbnails: "Grade de fotos",
  indexBuilding: "🔍 Criando o índice de busca… ",
  bgScanning: "Atualizando a biblioteca…",
  bgThumbs: (n: number) => `Lendo fotos (${one(n, "falta", "faltam")} ${num(n)})`,
  bgScanningShort: "Atualizando",
  bgThumbsShort: (n: number) => `${one(n, "falta", "faltam")} ${num(n)}`,
  cameraScanning: "📷 Lendo informações da câmera… ",
  indexIncompleteWarning:
    "⚠ A indexação da busca foi interrompida — os resultados podem estar incompletos (ela continua na próxima abertura)",
  indexProgressSuffix: "% — os resultados podem estar incompletos até terminar",
  removeRoot: (path: string) => `Remover ${path} da biblioteca`,
  rootMissingNotice: (name: string, count: number) =>
    `A pasta “${name}” não está lá${count > 0 ? ` (${num(count)} ${one(count, "foto nela não pode ser aberta", "fotos nela não podem ser abertas")})` : ""}. Se ela estiver em um pendrive ou cartão SD, conecte-o e pressione Atualizar.`,
  rootsMissingNotice: (names: string, count: number) =>
    `Algumas pastas não estão lá: ${names}${count > 0 ? ` (${num(count)} ${one(count, "foto nelas não pode ser aberta", "fotos nelas não podem ser abertas")})` : ""}. Se estiverem em um pendrive ou cartão SD, conecte-o de novo e pressione Atualizar. Para parar de usar uma pasta, remova-a da biblioteca com o ✕ na lista à esquerda (os arquivos das fotos não são apagados).`,
  rootRemoveFromLibrary: "Remover da biblioteca",
  rootRemoveKeepsFiles: "Apenas remove a pasta da biblioteca. Os arquivos das fotos não são apagados",
  rootRemoveConfirm: (name: string) =>
    `Remover “${name}” da biblioteca?\nOs arquivos das fotos não são apagados, mas as marcas ★ e ⚑ das fotos, sim.`,
  rootTempConfirm: (path: string) =>
    `“${path}” está dentro de uma pasta temporária.\nO sistema ou outros apps podem apagar arquivos ali sem perguntar. Adicionar à biblioteca mesmo assim?`,
  rootTempConfirmOk: "Adicionar mesmo assim",
  destTempConfirm: (path: string) =>
    `“${path}” está dentro de uma pasta temporária.\nAs fotos importadas ali podem ser apagadas pelo sistema ou por outros apps sem perguntar. Depois que o cartão for apagado, elas não existirão em nenhum outro lugar. Usar como destino mesmo assim?`,
  destTempConfirmOk: "Usar como destino",
  destTempWarning:
    "⚠ O destino está dentro de uma pasta temporária. As fotos importadas podem sumir sem aviso.",
  rootTempTip: (path: string) =>
    `${path} — dentro de uma pasta temporária. O sistema ou outros apps podem apagar arquivos ali sem perguntar`,
  rootMissingTempTip: (path: string) =>
    `${path} — não está lá. Estava dentro de uma pasta temporária, então o sistema ou outro app pode tê-la apagado`,
  rootMissingTip: (path: string) =>
    `${path} — não está lá. Se estiver em um pendrive ou cartão SD, conecte-o e pressione Atualizar`,
  importFrom: (path: string) => `Importar de ${path}`,
  filterByCamera: (name: string) => `Mostrar só as fotos tiradas com ${name}`,
  jumpToYear: (year: number) => `Ir para ${year}`,
  importing: (done: number, total: number) => `Importando… ${num(done)}/${num(total)}`,
  importDone: (copied: number, skipped: number) =>
    `Importação concluída: ${num(copied)} ${one(copied, "copiada", "copiadas")}, ${num(skipped)} ${one(skipped, "ignorada", "ignoradas")}`,
  importFailed: (n: number) => `, ${num(n)} com falha`,
  importIncomplete: " ⚠ Algumas pastas não puderam ser lidas — não apague o cartão ainda",
  syncDone: (added: number, changed: number, removed: number) =>
    `${num(added)} ${one(added, "adicionada", "adicionadas")}, ${num(changed)} ${one(changed, "alterada", "alteradas")}, ${num(removed)} ${one(removed, "removida", "removidas")}`,
  drivesReturnedScanned: (added: number, changed: number) =>
    `As pastas da unidade conectada foram lidas de novo (${num(added)} ${one(added, "adicionada", "adicionadas")}, ${num(changed)} ${one(changed, "alterada", "alteradas")})`,
  pickSource: "Escolha a pasta de origem da importação (USB / DCIM)",
  pickDestination: "Escolha a pasta de destino",
  wizardTitle: "Importar",
  wizardSources: "Origem",
  wizardOtherFolder: "Outra pasta…",
  wizardRefresh: "Atualizar unidades",
  wizardRemovable: "Removível",
  wizardNoDrives: "Nenhuma unidade encontrada",
  emptyTitle: "Ainda não há fotos",
  emptyTitleFailed: "Não foi possível mostrar a lista",
  emptyTitleChecking: "Verificando",
  emptyTitleStartupFailed: "A sincronização inicial não terminou",
  emptyStartupFailed:
    "A sincronização que roda ao iniciar não terminou. Pode haver fotos que ainda não foram incluídas. Pressione Atualizar; se não resolver, abra o app de novo.",
  emptyTitleMissing: "Alguns locais não estão lá",
  emptyTitleUnreadable: "Alguns locais não puderam ser abertos",
  emptyNoRoots:
    "Ainda não há nenhuma pasta na biblioteca. Importe de um cartão ou escolha uma pasta que tenha fotos.",
  emptyMissing: (names: string) =>
    `Estes locais não estão lá: ${names}. Se estiverem em um pendrive ou cartão SD, conecte-o de novo e pressione Atualizar.`,
  emptyUnreadableMac: (names: string) =>
    `Estes locais não puderam ser abertos: ${names}. Dê ao pictkura acesso a essa pasta (Mesa, Documentos, uma unidade externa) em Ajustes do Sistema → Privacidade e Segurança. Se estiver em uma rede, verifique se está conectada e pressione Atualizar.`,
  emptyUnreadableWin: (names: string) =>
    `Estes locais não puderam ser abertos: ${names}. Verifique as permissões da pasta. Se for uma unidade de rede, verifique se está conectada e pressione Atualizar.`,
  emptyUnreadableOther: (names: string) =>
    `Estes locais não puderam ser abertos: ${names}. Verifique se você tem permissão para lê-los e pressione Atualizar.`,
  listSeparator: ", ",
  listLastSeparator: " ",
  andMore: (n: number) => `e mais ${num(n)}`,
  emptyRootIsPackage:
    "Uma das pastas da biblioteca é a própria biblioteca do app Fotos. O pictkura não lê o interior dela de propósito, então nada virá dali. Escolha uma pasta que tenha fotos ou importe de um cartão.",
  emptyPhotoLibrary:
    "Só foi encontrada a biblioteca do app Fotos. Por padrão, o pictkura não lê o interior do Fotos — a maioria dos originais fica no iCloud, não neste Mac. Importe de um cartão ou escolha uma pasta que tenha fotos.",
  emptyManagedLibrary:
    "Só foram encontradas bibliotecas de gerenciadores de fotos (Fotos, iPhoto ou Aperture). O pictkura não lê o interior delas de propósito — indexaria o conteúdo uma vez e nunca mais veria as mudanças. Importe de um cartão ou escolha uma pasta que tenha fotos.",
  emptyRootIsManagedLibrary:
    "Uma das pastas da biblioteca é a própria biblioteca de um gerenciador de fotos (Fotos, iPhoto ou Aperture). O pictkura não lê o interior dela de propósito, então nada virá dali. Escolha uma pasta que tenha fotos ou importe de um cartão.",
  emptyAllExcluded: (names: string) =>
    `Tudo o que foi encontrado é ignorado pelos padrões de exclusão (por exemplo ${names}). Você pode alterá-los no pictkura.toml da pasta de configurações.`,
  emptyNothingHere:
    "Ainda não apareceu nenhuma foto que o pictkura consiga ler. Importe de um cartão ou escolha uma pasta que tenha fotos.",
  calendarChecking: "Verificando…",
  emptyTitleStalled: "Alguns locais não respondem",
  emptyStalled: (names: string) =>
    `Estes locais não respondem: ${names}. Se algum estiver em uma rede, verifique se ainda está conectado e pressione Atualizar. Se ele não existir mais, remova-o das pastas da biblioteca e os outros vão aparecer.`,
  emptyChecking:
    "Ainda procurando nas pastas. Se alguma estiver em uma rede, verifique se está conectada e pressione Atualizar.",
  emptyLoadFailed:
    "Não foi possível carregar a lista. Pressione Atualizar ou abra o app de novo.",
  wizardPickFolderHint: "Escolha uma pasta à esquerda para ver as fotos dela",
  wizardNoImages: "Não há fotos nesta pasta",
  wizardUnreadable: "Não foi possível ler esta pasta (ela pode ter sido removida)",
  wizardCounting: "Carregando…",
  wizardSelectAll: "Selecionar tudo",
  wizardSelectNew: "Selecionar só as novas",
  wizardClearSelection: "Limpar seleção",
  wizardSelected: (n: number) => `${num(n)} ${one(n, "selecionada", "selecionadas")}`,
  wizardImportedBadge: "✓",
  wizardImportedTitle: "Já importada (o mesmo arquivo existe no destino)",
  wizardUnsureBadge: "?",
  wizardUnsureTitle:
    "Este nome aparece mais de uma vez no cartão. Só dá para saber se o arquivo no destino é esta foto lendo o conteúdo — a importação lê, e não faz uma segunda cópia de uma foto que já está lá",
  wizardDestination: "Destino",
  wizardChangeDestination: "Alterar",
  wizardStructure: "Organização",
  wizardImportButton: (n: number) => `Importar ${num(n)}`,
  wizardImportAll: "Importar a pasta inteira (incluindo subpastas)",
  wizardImportAllShort: "Pasta inteira",
  wizardDeep: "Incluir subpastas",
  wizardDeepHint: "Percorre a mídia inteira, para você não precisar saber onde estão as fotos",
  wizardScanning: "Procurando na mídia…",
  wizardTruncated: (n: number) =>
    `Mostrando só as primeiras ${num(n)}. Use “Pasta inteira” para importar tudo`,
  wizardScanIncomplete: "⚠ Algumas pastas não puderam ser lidas (podem faltar fotos)",
  decoderHeifNotice: (n: number) =>
    `⚠ ${num(n)} ${one(n, "foto", "fotos")} HEIC/HEIF não ${one(n, "tem", "têm")} miniatura aqui e também não ${one(n, "abre", "abrem")}. São necessárias as Extensões de Imagem HEIF (gratuitas) e também as Extensões de Vídeo HEVC (pagas, alguns reais), que decodificam os pixels`,
  decoderHeifNoticeMac: (n: number) =>
    `⚠ ${num(n)} ${one(n, "foto", "fotos")} HEIC/HEIF não ${one(n, "tem", "têm")} miniatura aqui e também não ${one(n, "abre", "abrem")}`,
  decoderHeifNoticeOther: (n: number) =>
    `⚠ ${num(n)} ${one(n, "foto", "fotos")} HEIC/HEIF não ${one(n, "tem", "têm")} miniatura aqui e também não ${one(n, "abre", "abrem")}. Seu sistema pode não ter um decodificador para HEIC/HEVC`,
  decoderHeifHow: "Extensões de Imagem HEIF (gratuitas)",
  decoderHevcHow: "Extensões de Vídeo HEVC (pagas)",
  decoderNoticeDismiss: "Não mostrar de novo",
  wizardOfflineTitle:
    "Este arquivo está na nuvem (sem prévia aqui; a importação vai baixá-lo)",
  wizardHideImported: "Ocultar as já importadas",
  wizardAllImported: "Nada de novo aqui (tudo nesta pasta já foi importado)",
  wizardHiddenCount: (n: number) =>
    `${num(n)} já ${one(n, "importada oculta", "importadas ocultas")}`,
  wizardEtaSeconds: (n: number) => `cerca de ${num(n)} s ${one(n, "restante", "restantes")}`,
  wizardEtaMinutes: (n: number) => `cerca de ${num(n)} min ${one(n, "restante", "restantes")}`,
  wizardEtaCalculating: "calculando o tempo restante…",
  wizardCapped: (n: number) => `${num(n)}+`,
  wizardMoreFiles: (n: number) => `mais ${num(n)} (role para carregar)`,
  menuOpen: "Abrir",
  menuOpenWith: (name: string) => `Abrir com ${name}`,
  menuOpenWithOther: "Abrir com outro app…",
  menuReveal: "Mostrar na pasta",
  menuMoreOptions: "Mostrar mais opções",
  menuCopyImage: "Copiar imagem para a área de transferência",
  menuRotateRight: "Girar à direita",
  menuRotateLeft: "Girar à esquerda",
  menuRotateRestore: "Restaurar a orientação original",
  menuPrint: "Imprimir…",
  printTitle: "Imprimir",
  printFitContain: "Ajustar à página",
  printFitContainHint: "A foto cabe inteira. Se o formato for diferente do papel, sobram margens.",
  printFitCover: "Cortar para preencher",
  printFitCoverHint: "Sem margens, mas as bordas da foto que não cabem são cortadas.",
  printFitStretch: "Esticar para preencher (ignorar proporções)",
  printFitStretchHint: "Sem margens e sem cortes, mas a foto fica esticada.",
  printBorderless: "Para imprimir sem bordas, escolha um tamanho de papel sem bordas na próxima janela de impressão.",
  printGo: "Imprimir…",
  printPreparing: "Preparando a impressão…",
  printSheetTitle: (n: number) => `Imprimir ${num(n)} ${one(n, "foto", "fotos")}`,
  printPerPage: "Fotos por página",
  printCaptions: "Incluir nome do arquivo e data",
  printTooMany: (max: number) => `Neste tamanho, é possível imprimir até ${num(max)} fotos de uma vez. Escolha mais por página ou selecione menos.`,
  printPreparingN: (done: number, total: number) => `Preparando a impressão… ${num(done)}/${num(total)}`,
  printSkipped: (n: number) =>
    `${num(n)} ${one(n, "item não pôde ser impresso (vídeos e afins) e ficou de fora", "itens não puderam ser impressos (vídeos e afins) e ficaram de fora")}.`,
  errPrint: "Não foi possível preparar a foto para impressão.",
  errShellMenu: "Não foi possível mostrar o menu do Explorador de Arquivos.",
  menuDelete: "Apagar (mover para a lixeira)",
  menuInfo: "Informações",
  fileInfoTitle: "Arquivo",
  fileInfoName: "Nome",
  fileInfoFolder: "Pasta",
  fileInfoSize: "Tamanho",
  fileInfoTaken: "Tirada em",
  fileInfoTakenUnknown: "Desconhecida",
  fileInfoModified: "Modificado em",
  fileInfoDimensions: "Dimensões",
  fileInfoDuration: "Duração",
  fileInfoType: "Tipo",
  selectionTotal: (size: string) => `${size} no total`,
  menuFavoriteOn: "Adicionar às favoritas",
  menuFavoriteOff: "Remover das favoritas",
  pickEditor: "Escolha um app para editar",
  deleteConfirm: (n: number) =>
    n === 1
      ? "Mover esta foto para a lixeira?"
      : `Mover ${num(n)} fotos para a lixeira?`,
  deleted: (n: number) => `${num(n)} ${one(n, "movida", "movidas")} para a lixeira`,
  deletedSomeLeft: (n: number, left: number) =>
    `${num(n)} ${one(n, "movida", "movidas")} para a lixeira (${num(left)} não ${one(left, "foi encontrada e ficou", "foram encontradas e ficaram")} como ${one(left, "estava", "estavam")})`,
  deletedMissing: (n: number, missing: number) =>
    `${num(n)} ${one(n, "movida", "movidas")} para a lixeira (${num(missing)} já não ${one(missing, "estava", "estavam")} na biblioteca e não ${one(missing, "foi movida", "foram movidas")})`,
  selectItem: "Selecionar",
  selectedCount: (n: number) => `${num(n)} ${one(n, "selecionada", "selecionadas")}`,
  selectAll: "Selecionar tudo",
  clearSelection: "Limpar seleção (Esc)",
  selectDay: "Selecionar o dia inteiro",
  bulkFavoriteOn: "Adicionar às favoritas",
  bulkFavoriteOff: "Remover das favoritas",
  bulkDelete: "Mover para a lixeira",
  bulkCopy: "Copiar para pasta",
  bulkMove: "Mover para pasta",
  bulkViewer: "Ver a seleção",
  pickExportFolder: "Escolha uma pasta para exportar",
  pickMoveFolder: "Escolha uma pasta para onde mover",
  moveConfirm: (n: number) =>
    n === 1
      ? "Mover esta foto para uma pasta que você vai escolher a seguir?\nEla sai do lugar atual e sai da biblioteca.\nAs marcas ★ e ⚑ não são levadas junto."
      : `Mover ${num(n)} fotos para uma pasta que você vai escolher a seguir?\nElas saem do lugar atual e saem da biblioteca.\nAs marcas ★ e ⚑ não são levadas junto.`,
  confirmCancel: "Cancelar",
  deleteConfirmOk: "Mover para a lixeira",
  moveConfirmOk: "Escolher o destino",
  rootRemoveConfirmOk: "Remover",
  exporting: (done: number, total: number, name: string) =>
    `Exportando… ${num(done)}/${num(total)} ${name}`,
  exportDone: (done: number, skipped: number, failed: number, missing: number) => {
    const parts: string[] = [];
    if (done > 0 || skipped + failed + missing === 0)
      parts.push(`${num(done)} ${one(done, "foto exportada", "fotos exportadas")}`);
    if (skipped > 0) parts.push(`${num(skipped)} já ${one(skipped, "estava", "estavam")} lá`);
    if (failed > 0) parts.push(`${num(failed)} com falha`);
    if (missing > 0)
      parts.push(`${num(missing)} já não ${one(missing, "estava", "estavam")} na biblioteca e não ${one(missing, "foi exportada", "foram exportadas")}`);
    return parts.join(". ") + ".";
  },
  moving: (done: number, total: number, name: string) =>
    `Movendo… ${num(done)}/${num(total)} ${name}`,
  moveDone: (moved: number, skipped: number, failed: number, leftBehind: number, missing: number) => {
    const parts: string[] = [];
    if (moved > 0 || skipped + failed + leftBehind + missing === 0)
      parts.push(`${num(moved)} ${one(moved, "foto movida", "fotos movidas")}`);
    if (skipped > 0) parts.push(`${num(skipped)} já ${one(skipped, "estava lá e ficou", "estavam lá e ficaram")} no lugar`);
    if (failed > 0) parts.push(`${num(failed)} não ${one(failed, "pôde ser movida e ficou", "puderam ser movidas e ficaram")} no lugar`);
    if (leftBehind > 0)
      parts.push(`${num(leftBehind)} ${one(leftBehind, "copiada, mas não pôde ser removida", "copiadas, mas não puderam ser removidas")} do lugar original`);
    if (missing > 0)
      parts.push(`${num(missing)} já não ${one(missing, "estava", "estavam")} na biblioteca e não ${one(missing, "foi movida", "foram movidas")}`);
    return parts.join(". ") + ".";
  },
  bulkPickOn: "Escolher",
  bulkPickOff: "Desfazer escolha",
  bulkPickDone: (n: number) => `${num(n)} ${one(n, "foto escolhida", "fotos escolhidas")}`,
  bulkUnpickDone: (n: number) =>
    `Escolha desfeita em ${num(n)} ${one(n, "foto", "fotos")}`,
  bulkFavoriteDone: (n: number) =>
    `${num(n)} ${one(n, "foto adicionada", "fotos adicionadas")} às favoritas`,
  bulkUnfavoriteDone: (n: number) =>
    `${num(n)} ${one(n, "foto removida", "fotos removidas")} das favoritas`,
  settings: "Configurações",
  close: "Fechar",
  settingsTitle: "Configurações",
  settingsImportStructure: "Estrutura das pastas de importação",
  settingsImportStructureNote:
    "Como as fotos importadas são organizadas pela data de captura. Isso afeta milhares de arquivos, e mudar depois dá bastante trabalho. Quando o nome de uma pasta tem data, ela é escrita com o ano primeiro em todos os idiomas, para que ordenar por nome seja ordenar por data.",
  settingsDestination: "Destino",
  settingsDestinationUnset: "(não definido — você escolhe na primeira importação)",
  settingsFlatExample: "IMG_0001.JPG (sem subpastas)",
  settingsCustomPattern: "Personalizado",
  settingsCustomPatternNote:
    "{year} {month} {day} são trocados pela data. Use / para criar níveis. Caracteres inválidos e subidas para a pasta pai (..) são removidos automaticamente.",
  settingsCustomPatternResult: "Pasta resultante",
  settingsGrid: "Grade de fotos",
  settingsStackRawJpegToggle: "Empilhar pares RAW+JPEG em um só bloco",
  settingsStackRawJpegNote:
    "Um RAW e um JPEG com o mesmo nome na mesma pasta aparecem como um só bloco, o JPEG. Na grade, ★, ⚑, apagar e a seleção valem para os dois. Qual arquivo a tela cheia mostra é definido em “Ao ver uma foto em tela cheia”.",
  stackRawChipTitle: (files: number) =>
    `RAW e JPEG empilhados em um só bloco (${num(files)} arquivos)`,
  settingsStackBurstsToggle: "Empilhar sequências em um só bloco",
  settingsStackBurstsNote:
    "Fotos tiradas em sequência rápida com a mesma câmera aparecem como um só bloco. Só são empilhadas as fotos cujo horário de captura é registrado em frações de segundo (algumas câmeras não registram). Na grade, ★ e ⚑ do menu do clique direito valem só para a foto de capa; apagar e a seleção valem para a sequência inteira. A tela cheia não junta as sequências; ela passa por cada foto.",
  settingsBurstGap: "Intervalo máximo dentro de uma sequência",
  settingsOutgoing: "Saída",
  settingsGoogleToggle: "Colocar as fotos importadas também na pasta de Saída",
  settingsGoogleNote: "Cada importação também coloca as fotos na pasta de Saída (sem ocupar espaço extra no disco). Dos pares RAW+JPEG, só o JPEG entra. Se você usa o Google Fotos: adicione esta pasta ao backup de pastas do Google Fotos.",
  settingsGoogleFolder: "Pasta",
  settingsGoogleOpenFolder: "Abrir pasta",
  settingsGoogleCopyPath: "Copiar caminho",
  settingsGoogleCopied: "Copiado",
  settingsGoogleCopyFailed: "Não foi possível copiar. Selecione o caminho e copie-o.",
  settingsGoogleFolderNote: "Ela fica na mesma unidade que o destino da importação. Escolha um lugar fora de qualquer pasta vigiada por outro backup, como um NAS ou o Amazon Photos.",
  settingsGoogleFoldersNote: "Pastas de Saída em que o pictkura colocou fotos (é criada uma por unidade). Se você usa o Google Fotos: adicione ao backup de pastas cada uma que você usa.",
  settingsGoogleVideo: "Colocar vídeos também",
  settingsGoogleRawOnly: "Fotos só em RAW",
  googleRawOnlyNone: "Deixar de fora",
  googleRawOnlyEmbedded: "Colocar o JPEG embutido",
  settingsGoogleRawOnlyNote: "O que fazer com arquivos RAW tirados sem JPEG. “JPEG embutido” extrai a imagem de prévia que a câmera guardou dentro do RAW e a coloca com a data de captura. O JPEG extraído é um arquivo à parte, então ocupa espaço no disco.",
  settingsGoogleOneDrive: "Colocar também as fotos que estão no OneDrive",
  settingsGoogleOneDriveNote: "Se fizer isso, o OneDrive não poderá mais deixar essas fotos só on-line.",
  settingsGoogleAfterNote: "Quando você move uma foto para a lixeira no pictkura, ela também sai da pasta de Saída. Se você usa o Google Fotos: ela continua no Google Fotos. O envio é feito pela página web do Google Fotos (photos.google.com), então só avança enquanto essa página estiver aberta no navegador.",
  pickGoogleFolder: "Escolha a pasta de Saída",
  importGoogle: (n: number) => ` · ${num(n)} ${one(n, "colocada", "colocadas")} na Saída`,
  importGoogleFailed: (n: number) =>
    `${num(n)} não ${one(n, "pôde ser colocada", "puderam ser colocadas")} na Saída (veja o registro)`,
  importGoogleError: (why: string) => `Nada foi colocado na Saída: ${why}`,
  bulkSendGoogle: "Adicionar à Saída",
  bulkRemoveGoogle: "Remover da Saída",
  calendarSendMonth: "Adicionar este mês à Saída",
  calendarSendYear: (year: string) => `Adicionar todo o ano ${year} à Saída`,
  googleSendSummary: (photos: number, videos: number, rawOnly: number, size: string) => {
    const parts: string[] = [];
    if (photos > 0) parts.push(`${num(photos)} ${one(photos, "foto", "fotos")}`);
    if (videos > 0) parts.push(`${num(videos)} ${one(videos, "vídeo", "vídeos")}`);
    const items: string[] = [];
    if (parts.length > 0) items.push(`${parts.join(", ")}${size ? ` (${size})` : ""}`);
    if (rawOnly > 0)
      items.push(`${num(rawOnly)} ${one(rawOnly, "JPEG extraído", "JPEGs extraídos")} de fotos só em RAW`);
    return `Será colocado na pasta de Saída: ${items.join("; ")}.`;
  },
  googleSendLeftOut: (n: number) =>
    `${num(n)} dos itens escolhidos ${one(n, "fica", "ficam")} de fora (o RAW de um par RAW+JPEG, arquivos dentro do OneDrive e afins).`,
  googleSendFolders: (list: string) => `Colocado em: ${list}`,
  googleSendNewFolder: (path: string) => `${path} será criada.`,
  googleSendGoogleNote:
    "Se você usa o Google Fotos: o envio é feito pela página web do Google Fotos (photos.google.com), então avança enquanto essa página estiver aberta no navegador.",
  googleSendGoogleNoteNew:
    "Se você usa o Google Fotos: adicione também cada pasta nova ao backup de pastas do Google Fotos. O envio é feito pela página web do Google Fotos (photos.google.com), então avança enquanto essa página estiver aberta no navegador.",
  googleFolderCreated: (path: string) =>
    `${path} foi criada. Se você usa o Google Fotos: adicione também esta pasta ao backup de pastas.`,
  googleSendNothing: "Nada aqui pode ser colocado.",
  googleSendConfirmOk: "Colocar",
  googleSent: (total: number, already: number) =>
    `${num(total)} ${one(total, "colocado", "colocados")} na Saída${already > 0 ? ` (${num(already)} já ${one(already, "estava", "estavam")} lá)` : ""}`,
  googleSentCloudOnly: (n: number) =>
    `${num(n)} ${one(n, "está", "estão")} só na nuvem e não ${one(n, "pôde ser colocado", "puderam ser colocados")}. ${one(n, "Baixe-o", "Baixe-os")} primeiro e envie de novo.`,
  googleRemoved: (n: number) => `${num(n)} ${one(n, "removido", "removidos")} da Saída`,
  googleRemoveFailed: (n: number) =>
    `${num(n)} não ${one(n, "pôde ser removido", "puderam ser removidos")} agora (a unidade pode estar desconectada). O pictkura vai tentar de novo na próxima vez que iniciar.`,
  googleSentLeftOut: (n: number) =>
    `${num(n)} ${one(n, "ficou", "ficaram")} de fora (o RAW de um par RAW+JPEG, arquivos dentro do OneDrive e afins).`,
  burstGapOption: (seconds: number) => `${num(seconds)} s`,
  burstChip: (frames: number) => `▤ Sequência ${num(frames)}`,
  burstChipShort: (frames: number) => `▤ ${num(frames)}`,
  spreadStack: "Espalhar para ver cada uma",
  foldStack: "Recolher",
  burstTitle: (frames: number, spanMs: number, files: number) =>
    `Sequência de ${num(frames)} fotos em ${num(Math.round(spanMs / 100) / 10)} s (${num(files)} arquivos)`,
  burstTitleRawJpeg: (frames: number, spanMs: number, files: number) =>
    `Sequência de ${num(frames)} fotos em ${num(Math.round(spanMs / 100) / 10)} s, RAW+JPEG (${num(files)} arquivos)`,
  settingsViewer: "Ao ver uma foto em tela cheia",
  settingsAutoAdvanceToggle: "Passar para a próxima foto depois de P / U",
  settingsAutoAdvanceNote:
    "Em tela cheia, P marca a foto com ⚑ (uma prateleira separada das favoritas ★) e U a limpa. Com isto ligado, a próxima foto vem logo em seguida, então escolher custa uma tecla por foto. Desligado, você fica na mesma foto.",
  settingsPairView: "O que mostrar de um par RAW+JPEG",
  pairViewJpeg: "Só o JPEG",
  pairViewRaw: "Só o RAW",
  pairViewBoth: "Os dois (RAW, depois JPEG)",
  settingsPairViewNote:
    "Como a tela cheia passa por um par mostrado como um só bloco na grade. Mostrando um lado, ★, ⚑, ✕ e apagar em tela cheia valem para os dois arquivos do par. Mostrando os dois, valem só para o arquivo que você está vendo. Os botões RAW / JPEG da tela cheia também alternam. Se os pares não estiverem empilhados na grade, os dois arquivos aparecem.",
  viewerPairSide: "Qual arquivo do par mostrar (também fica salvo nas configurações)",
  settingsTransition: "Ao passar para a próxima foto",
  settingsSlideshowTransition: "Transição da apresentação",
  transitionNone: "Nenhuma",
  transitionSlide: "Deslizar",
  transitionFade: "Esmaecer",
  settingsTransitionNote:
    "Como a tela cheia passa para a próxima foto quando você avança (setas, botões, a faixa embaixo ou o avanço depois de um sinalizador) e quando a apresentação de slides avança. Deslizar segue a direção em que você avança. Se o sistema estiver configurado para reduzir o movimento, as fotos trocam sem se mover, seja qual for a escolha aqui.",
  settingsAutoplay: "Ao conectar um pendrive ou cartão SD",
  settingsAutoplayNote:
    "Se o pictkura foi instalado com o instalador ou pela Microsoft Store, “pictkura で写真を取り込む” (importar fotos com o pictkura) aparece entre as opções quando você conecta uma unidade; a versão portátil não oferece isso. Nada abre até você escolher. Depois disso, o Windows guarda a escolha como padrão para esse tipo de mídia (cartões de memória e unidades USB são configurados separadamente) e abre a janela de importação sempre que você conectar uma do mesmo tipo. O que acontece por padrão é alterado nas configurações de Reprodução Automática do Windows. A opção em si está escrita em japonês, exceto com o MSI em inglês, em que aparece como “Import photos with pictkura”.",
  settingsAutoplayOpenWindows: "Abrir as configurações de Reprodução Automática do Windows",
  settingsImportWaitingFrom: (path: string) =>
    `Uma importação de ${path} está esperando. Ela abre quando você fechar as Configurações.`,
  settingsImportWaiting: "Uma importação está esperando. Ela abre quando você fechar as Configurações.",
  settingsImportWaitingOpen: "Fechar e importar",
  settingsAbout: "Sobre",
  settingsAboutLicense: "Distribuído sob a licença MIT.",
  settingsManual: "Manual",
  settingsOssLicenses: "Código aberto que usamos",
  settingsDocNotBundled: "(não incluído numa build de desenvolvimento)",
  settingsLog: "Abrir o registro",
  settingsLogNone: "(nada registrado ainda)",
  settingsOpenFailed: "Não foi possível abrir.",
  settingsLogNote:
    "Uma linha só é escrita quando algo falha, e fica nesta máquina. Nada é enviado.",
  settingsLanguage: "Idioma",
  settingsLanguageSystem: "Igual ao sistema",
  settingsLanguageNote:
    "Trocar recarrega a janela. A cópia em si continua em segundo plano, mas o assistente de importação fecha e você perde de vista o andamento — então espere a importação terminar antes de trocar.",
  settingsLanguageMacNote:
    "As janelas do macOS, como o painel Salvar e as caixas de confirmação, passam para este idioma na próxima vez que você abrir o pictkura.",
  settingsTheme: "Tema",
  themeSystem: "Igual ao sistema",
  themeLight: "Claro",
  themeDark: "Escuro",
  settingsEditors: "Apps de edição",
  settingsEditorsNote: "Apps que você escolheu em “Abrir com outro app…”.",
  settingsForgetEditor: "Remover da lista",
  calendarEmpty: "Sem fotos",
  filterEmptyTitle: (conds: string) => `Nenhuma foto corresponde a estes filtros: ${conds}`,
  filterEmptyMessage: "Limpe a busca ou os filtros à esquerda para ver o resto.",
  filterEmptyStillIndexing: "Ainda há fotos sendo importadas ou indexadas, então podem aparecer mais.",
  filterCondQuery: (q: string) => `a busca “${q}”`,
  filterCondCamera: (name: string) => `tiradas com ${name}`,
  filterEmptyClearSearch: "Limpar busca",
  errGoogleOverlapsRoot: "Não é possível escolher uma pasta da biblioteca, uma pasta dentro dela ou uma pasta que a contenha.",
  errGoogleInsideSync: "Não é possível escolher um lugar dentro de uma pasta sincronizada (OneDrive, Google Drive, Dropbox, iCloud Drive e afins).",
  errGoogleNoHardLinks: "Esta unidade não suporta isso (exFAT, FAT e similares não criam links físicos).",
  errGoogleWholeVolume: "O destino da importação é uma unidade inteira, então não há onde colocar a pasta de Saída nela. Altere o destino da importação.",
  errGoogleNoName: "Não é possível escolher a própria unidade. Escolha uma pasta nela.",
  errGoogleLinkInTheWay: "A pasta de Saída é um link simbólico. Escolha outro lugar.",
  errGoogleIo: "Não foi possível ler ou gravar a pasta de Saída.",
  errGoogleOtherDrive: "Escolha uma pasta na mesma unidade que o destino da importação.",
  errNotFound: "Essa foto não está mais no índice.",
  errRotateNoOrientation: "Esta foto não tem informação de orientação, então não pode ser girada.",
  errRotateXmp: "Isso entraria em conflito com a orientação que os editores de fotos leem do XMP, então ela não pode ser girada.",
  errRotateCloudOnly: "Esta foto está só na nuvem, então não pode ser girada. Baixe-a e tente de novo.",
  errRotateCameraCard: "As fotos no cartão da câmera ficam como estão, porque é de lá que você importa. Importe-a e depois gire.",
  errRotateNotReady: "Ainda lendo esta foto. Espere um pouco e tente de novo.",
  errRotateChanged: "O arquivo foi alterado em outro lugar enquanto era girado. Tente de novo.",
  errRotateBusy: "Verificando as suas pastas. Espere um pouco e tente de novo.",
  errRotateWrite: "Não foi possível gravar no arquivo, então ela não foi girada. Verifique se nenhum outro app está com ele aberto e se ele não é somente leitura.",
  errRotateLeftChanged: "Algo deu errado depois que o arquivo foi regravado, e não foi possível desfazer. A foto pode continuar girada. Veja o registro para mais detalhes.",
  errRotateCannotRestore: "Esta foto não pode mais voltar à orientação de antes de o pictkura girá-la, por exemplo porque outro app a alterou.",
  errDb: "Não foi possível ler ou gravar o índice.",
  errConfigIo: "Não foi possível ler ou gravar o arquivo de configurações.",
  errConfigFormat: "Não foi possível entender o arquivo de configurações.",
  errNoDestination: "Ainda não foi escolhida uma pasta de destino.",
  errRootManaged:
    "Uma biblioteca gerenciada não pode ser adicionada como pasta — o que está dentro pertence a esse app.",
  errDestManaged:
    "Uma biblioteca gerenciada não pode ser o destino — o que está dentro pertence a esse app.",
  errNoImageToExtract: "Não há imagem neste arquivo para extrair.",
  errSourceUnreadable: "Não foi possível ler essa pasta.",
  errSourceManaged:
    "O interior de uma biblioteca gerenciada não pode ser importado. Tire a extensão (.photoslibrary e afins) do nome da pasta e escolha-a de novo.",
  errExportDest: "Não foi possível criar essa pasta de destino.",
  errExportManaged: "Nada pode ser gravado ali — outro app gerencia esse pacote.",
  errFolderMissing: "Essa pasta não está lá.",
  fileOpBusy: "Espere a operação atual terminar e tente de novo.",
  errFolderNotAbsolute: "Digite o caminho completo da pasta.",
  errTrashFailed: "Não foi possível mover para a lixeira.",
  errTrashUnreachable: "Não é possível acessar a unidade ou a pasta, então nada foi movido para a lixeira.",
  errTrashPartly: (n: number) =>
    `${num(n)} ${one(n, "movido", "movidos")} para a lixeira; o resto não pôde ser movido.`,
  errNoLogYet: "Nada foi registrado ainda.",
  errOutgoingFolderMissing: "A pasta de Saída não está lá. Ligar a Saída a cria (se já estiver ligada, desligue e ligue de novo).",
  errOpenOutgoingFailed: "Não foi possível abrir a pasta de Saída. Use “Copiar caminho” e abra-a por lá.",
  errNotBundled: "Não está incluído nesta build.",
  errNoStoreLink: "Este sistema não tem uma página para indicar.",
  errBadKind: "Não havia nada para abrir.",
  errTooManyIds: (n: number) => `No máximo ${num(n)} de cada vez.`,
};
