import { createContext, useContext, useState, useCallback, type ReactNode } from "react";

export type Lang = "en" | "zh" | "fr" | "es" | "ru";

const LANGS: Lang[] = ["zh", "en", "fr", "es", "ru"];

const LANG_LABELS: Record<Lang, string> = {
  zh: "中",
  en: "EN",
  fr: "FR",
  es: "ES",
  ru: "RU",
};

const DICT: Record<Lang, Record<string, string>> = {
  zh: {
    logo: "Moment",
    nav_create: "创作",
    nav_playlist: "发现",
    sign_in: "登录",
    tab_guided: "引导创作",
    tab_free: "自由创作",
    greet: "给 TA 做一首专属的歌",
    relationship_q: "TA 是你的什么人？",
    rel_kid: "孩子",
    rel_partner: "伴侣",
    rel_friend: "朋友",
    rel_parent: "父母",
    rel_pet: "宠物",
    rel_custom: "其他…",
    rel_custom_placeholder: "自己说，比如 同事、外甥女、自己…",
    name_q: "叫什么名字？或者你们之间怎么叫 TA？",
    name_placeholder: "小名、昵称都行…",
    name_ok: "好",
    gender_q: "TA 是…",
    gender_male: "男生",
    gender_female: "女生",
    bday_q: "TA 的生日是哪天？",
    bday_skip: "不知道，跳过",
    months: "1月,2月,3月,4月,5月,6月,7月,8月,9月,10月,11月,12月",
    scenario_q: "最近有什么让你印象深的时刻？",
    scenario_custom: "自己说…",
    scenario_custom_placeholder: "说说那个时刻…",
    lyrics_ready: "歌词写好了，看看满意吗？",
    review_header: "看看歌词",
    review_sub: "觉得合适就生成，也可以改",
    review_style: "曲风",
    review_vocal: "人声",
    review_vocal_female: "女声",
    review_vocal_male: "男声",
    review_title: "歌名",
    review_title_ph: "给这首歌起个名字",
    review_submit: "生成这首歌",
    instrumental_btn: "🎵 纯音乐生成",
    instrumental_q1: "想要什么样的情绪？",
    instrumental_q2: "喜欢什么乐器？",
    instrumental_q3: "节奏快慢？",
    instrumental_q4: "还有什么想补充的吗？",
    instrumental_q4_ph: "任何想法…",
    instrumental_submit: "生成",
    instrumental_skip: "跳过",
    paste_lyrics_btn: "我有歌词，直接生成 →",
    paste_lyrics_q: "把你的歌词粘贴进来，我帮你整理结构",
    paste_lyrics_ph: "粘贴歌词…",
    paste_lyrics_submit: "整理歌词",
    generating: "创作中…",
    ready_title: "歌曲创作完成！",
    ready_sub: "点击试听",
    gen_failed: "生成失败",
    gen_retry: "点击重试",
    free_lyrics: "歌词",
    free_instrumental: "纯音乐",
    free_style: "风格",
    free_style_ph: "输入风格、情绪、乐器等",
    free_vocal: "人声",
    free_title: "歌名",
    free_title_ph: "歌名",
    free_polish: "优化",
    free_expand: "生成歌词",
    free_generate: "🎵 生成歌曲",
    free_generating: "生成中…",
  },
  en: {
    logo: "Moment",
    nav_playlist: "Playlist",
    sign_in: "Sign in",
    tab_guided: "Guided",
    tab_free: "Free",
    greet: "Let's create a song for someone special",
    relationship_q: "Who are they to you?",
    rel_kid: "My Child",
    rel_partner: "Partner",
    rel_friend: "Friend",
    rel_parent: "Parent",
    rel_pet: "Pet",
    rel_custom: "Other…",
    rel_custom_placeholder: "e.g. colleague, niece, myself…",
    name_q: "What's their name? Or what do you call them?",
    name_placeholder: "Nickname or pet name…",
    name_ok: "OK",
    gender_q: "They are…",
    gender_male: "Male",
    gender_female: "Female",
    bday_q: "When is their birthday?",
    bday_skip: "Don't know, skip",
    months: "Jan,Feb,Mar,Apr,May,Jun,Jul,Aug,Sep,Oct,Nov,Dec",
    scenario_q: "Any special moment you remember?",
    scenario_custom: "Tell me…",
    scenario_custom_placeholder: "Describe that moment…",
    lyrics_ready: "Here's what I came up with. Feel free to edit!",
    review_header: "Review lyrics",
    review_sub: "Edit anything before creating",
    review_style: "Style",
    review_vocal: "Vocal",
    review_vocal_female: "Female",
    review_vocal_male: "Male",
    review_title: "Title",
    review_title_ph: "Give your song a name",
    review_submit: "Create Song",
    instrumental_btn: "🎵 Just instrumental",
    instrumental_q1: "What mood?",
    instrumental_q2: "Favorite instruments?",
    instrumental_q3: "Tempo?",
    instrumental_q4: "Anything else to add?",
    instrumental_q4_ph: "Any extra thoughts…",
    instrumental_submit: "Create",
    instrumental_skip: "Skip",
    paste_lyrics_btn: "I have lyrics, skip to generate →",
    paste_lyrics_q: "Paste your lyrics here, I'll help structure them",
    paste_lyrics_ph: "Paste lyrics…",
    paste_lyrics_submit: "Structure lyrics",
    generating: "Creating…",
    ready_title: "Your song is ready!",
    ready_sub: "Tap to open",
    gen_failed: "Generation failed",
    gen_retry: "Tap to retry",
    free_lyrics: "Lyrics",
    free_instrumental: "Instrumental",
    free_style: "Style",
    free_style_ph: "Enter styles, moods, instruments…",
    free_vocal: "Vocal",
    free_title: "Title",
    free_title_ph: "Song title",
    free_polish: "Polish",
    free_expand: "Expand",
    free_generate: "🎵 Create Song",
    free_generating: "Generating…",
  },
  fr: {
    logo: "Moment",
    nav_create: "Créer",
    nav_playlist: "Playlist",
    sign_in: "Se connecter",
    tab_guided: "Guidé",
    tab_free: "Libre",
    greet: "Créons une chanson pour quelqu'un de spécial",
    relationship_q: "Qui est cette personne pour toi ?",
    rel_kid: "Enfant",
    rel_partner: "Partenaire",
    rel_friend: "Ami(e)",
    rel_parent: "Parent",
    rel_pet: "Animal",
    rel_custom: "Autre…",
    rel_custom_placeholder: "ex. collègue, nièce, moi-même…",
    name_q: "Quel est son nom ?",
    name_placeholder: "Surnom…",
    name_ok: "OK",
    gender_q: "C'est…",
    gender_male: "Homme",
    gender_female: "Femme",
    bday_q: "Quelle est sa date d'anniversaire ?",
    bday_skip: "Je ne sais pas",
    months: "Jan,Fév,Mar,Avr,Mai,Juin,Juil,Août,Sep,Oct,Nov,Déc",
    scenario_q: "Un moment spécial en tête ?",
    scenario_custom: "Dis-moi…",
    scenario_custom_placeholder: "Décris ce moment…",
    lyrics_ready: "Voici ce que j'ai écrit. Tu peux modifier !",
    review_header: "Vérifier les paroles",
    review_sub: "Modifie avant de créer",
    review_style: "Style",
    review_vocal: "Voix",
    review_vocal_female: "Féminine",
    review_vocal_male: "Masculine",
    review_title: "Titre",
    review_title_ph: "Donne un nom à ta chanson",
    review_submit: "Créer",
    instrumental_btn: "🎵 Instrumental",
    paste_lyrics_btn: "J'ai des paroles →",
    paste_lyrics_q: "Colle tes paroles ici",
    paste_lyrics_ph: "Colle les paroles…",
    paste_lyrics_submit: "Structurer",
    generating: "Création…",
    ready_title: "Ta chanson est prête !",
    ready_sub: "Appuie pour écouter",
    gen_failed: "Échec",
    gen_retry: "Réessayer",
    free_lyrics: "Paroles",
    free_instrumental: "Instrumental",
    free_style: "Style",
    free_style_ph: "Styles, ambiances, instruments…",
    free_vocal: "Voix",
    free_title: "Titre",
    free_title_ph: "Titre",
    free_polish: "Peaufiner",
    free_expand: "Développer",
    free_generate: "🎵 Créer",
    free_generating: "Création…",
  },
  es: {
    logo: "Moment",
    nav_playlist: "Playlist",
    sign_in: "Iniciar sesión",
    tab_guided: "Guiado",
    tab_free: "Libre",
    greet: "Creemos una canción para alguien especial",
    relationship_q: "¿Quién es para ti?",
    rel_kid: "Hijo/a",
    rel_partner: "Pareja",
    rel_friend: "Amigo/a",
    rel_parent: "Padre/Madre",
    rel_pet: "Mascota",
    rel_custom: "Otro…",
    rel_custom_placeholder: "ej. colega, sobrina, yo…",
    name_q: "¿Cómo se llama?",
    name_placeholder: "Apodo…",
    name_ok: "OK",
    gender_q: "Es…",
    gender_male: "Hombre",
    gender_female: "Mujer",
    bday_q: "¿Cuándo es su cumpleaños?",
    bday_skip: "No sé",
    months: "Ene,Feb,Mar,Abr,May,Jun,Jul,Ago,Sep,Oct,Nov,Dic",
    scenario_q: "¿Algún momento especial?",
    scenario_custom: "Cuéntame…",
    scenario_custom_placeholder: "Describe ese momento…",
    lyrics_ready: "¡Aquí está! Puedes editarlo.",
    review_header: "Revisar letra",
    review_sub: "Edita antes de crear",
    review_style: "Estilo",
    review_vocal: "Voz",
    review_vocal_female: "Femenina",
    review_vocal_male: "Masculina",
    review_title: "Título",
    review_title_ph: "Ponle un nombre",
    review_submit: "Crear",
    instrumental_btn: "🎵 Instrumental",
    paste_lyrics_btn: "Tengo letra →",
    paste_lyrics_q: "Pega tu letra aquí",
    paste_lyrics_ph: "Pega la letra…",
    paste_lyrics_submit: "Estructurar",
    generating: "Creando…",
    ready_title: "¡Tu canción está lista!",
    ready_sub: "Toca para escuchar",
    gen_failed: "Falló",
    gen_retry: "Reintentar",
    free_lyrics: "Letra",
    free_instrumental: "Instrumental",
    free_style: "Estilo",
    free_style_ph: "Estilos, estados, instrumentos…",
    free_vocal: "Voz",
    free_title: "Título",
    free_title_ph: "Título",
    free_polish: "Pulir",
    free_expand: "Expandir",
    free_generate: "🎵 Crear",
    free_generating: "Creando…",
  },
  ru: {
    logo: "Moment",
    nav_playlist: "Плейлист",
    sign_in: "Войти",
    tab_guided: "Помощник",
    tab_free: "Сам",
    greet: "Давай создадим песню для особенного человека",
    relationship_q: "Кем он тебе приходится?",
    rel_kid: "Ребёнок",
    rel_partner: "Партнёр",
    rel_friend: "Друг",
    rel_parent: "Родитель",
    rel_pet: "Питомец",
    rel_custom: "Другое…",
    rel_custom_placeholder: "напр. коллега, племянница, я сам…",
    name_q: "Как его зовут?",
    name_placeholder: "Имя или прозвище…",
    name_ok: "ОК",
    gender_q: "Это…",
    gender_male: "Мужчина",
    gender_female: "Женщина",
    bday_q: "Когда у него день рождения?",
    bday_skip: "Не знаю",
    months: "Янв,Фев,Мар,Апр,Май,Июн,Июл,Авг,Сен,Окт,Ноя,Дек",
    scenario_q: "Какой особенный момент вспоминаешь?",
    scenario_custom: "Расскажи…",
    scenario_custom_placeholder: "Опиши этот момент…",
    lyrics_ready: "Вот что получилось. Можно редактировать!",
    review_header: "Проверить текст",
    review_sub: "Отредактируй перед созданием",
    review_style: "Стиль",
    review_vocal: "Вокал",
    review_vocal_female: "Женский",
    review_vocal_male: "Мужской",
    review_title: "Название",
    review_title_ph: "Дай песне имя",
    review_submit: "Создать",
    instrumental_btn: "🎵 Инструментал",
    paste_lyrics_btn: "У меня есть текст →",
    paste_lyrics_q: "Вставь текст песни",
    paste_lyrics_ph: "Вставь текст…",
    paste_lyrics_submit: "Структурировать",
    generating: "Создаётся…",
    ready_title: "Песня готова!",
    ready_sub: "Нажми, чтобы послушать",
    gen_failed: "Ошибка",
    gen_retry: "Повторить",
    free_lyrics: "Текст",
    free_instrumental: "Инструментал",
    free_style: "Стиль",
    free_style_ph: "Стили, настроения, инструменты…",
    free_vocal: "Вокал",
    free_title: "Название",
    free_title_ph: "Название",
    free_polish: "Улучшить",
    free_expand: "Расширить",
    free_generate: "🎵 Создать",
    free_generating: "Создаётся…",
  },
};

function resolveLang(): Lang {
  try {
    const stored = localStorage.getItem("moment_lang");
    if (stored && LANGS.includes(stored as Lang)) return stored as Lang;
  } catch {
    // localStorage unavailable
  }
  const nav = navigator.language.slice(0, 2);
  if (LANGS.includes(nav as Lang)) return nav as Lang;
  return "en";
}

export function getMonths(lang: Lang): string[] {
  const raw = DICT[lang]?.months ?? DICT.en.months;
  return raw.split(",");
}

interface I18nContextValue {
  lang: Lang;
  setLang: (l: Lang) => void;
  t: (key: string, params?: Record<string, string | number>) => string;
}

const I18nContext = createContext<I18nContextValue>({
  lang: "en",
  setLang: () => {},
  t: (k: string) => k,
});

export function I18nProvider({ children }: { children: ReactNode }) {
  const [lang, setLangState] = useState<Lang>(resolveLang);

  const setLang = useCallback((l: Lang) => {
    setLangState(l);
    try {
      localStorage.setItem("moment_lang", l);
    } catch {
      // ignore
    }
  }, []);

  const t = useCallback(
    (key: string, params?: Record<string, string | number>): string => {
      let text = DICT[lang]?.[key] ?? DICT.en[key] ?? key;
      if (params) {
        for (const [k, v] of Object.entries(params)) {
          text = text.replace(`{${k}}`, String(v));
        }
      }
      return text;
    },
    [lang],
  );

  return (
    <I18nContext.Provider value={{ lang, setLang, t }}>
      {children}
    </I18nContext.Provider>
  );
}

export function useI18n(): I18nContextValue {
  return useContext(I18nContext);
}

export { LANGS, LANG_LABELS };
