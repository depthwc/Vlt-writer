//! Pages to write by hand, on any paper. Their content is built for the cutter, not for
//! reading. The joined pages come from real text (news and encyclopedia corpora, 100K
//! sentences a language): a weighted cover of the letter pairs people write, word starts and
//! ends, capitals and the most frequent words, so that nearly every join of real text has a
//! version written by the person, most of them twice. Pages of one script share their joins
//! (Uzbek and English, Russian and Uzbek Cyrillic): the second language adds what the first
//! lacks. A page of characters written apart covers people who do not always join, and
//! English, which most people print, has a page of words with their letters apart. The
//! person copies a page line for line and photographs it; the text is known.

use std::collections::{HashMap, HashSet};

use serde::Serialize;

use crate::charset::{self, Lang};
use crate::hand::{Hand, Src};

/// How a page is to be written.
#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    /// each line in one go, joined as the person always writes
    Joined,
    /// words with their letters apart, as people print (English)
    Print,
    /// every character on its own, with a gap
    Separate,
    /// formulas, with gaps between the parts
    Math,
    /// words whose letters or joins did not come through
    Extra,
    /// only the characters not written yet, each a few times apart
    Fill,
}

// pages:begin (generated from word-frequency corpora, do not edit by hand)
const UZ_LATN_JOINED: &[&[&str]] = &[
    &[
        "xizmat byudjet avlodga asosida bogʻliq",
        "Elektr shu Sun'iy asosan boʻldi drenaj",
        "majmui Loyiha suv Husayn import mumkin",
        "Fors duo Juda uzum nutq mehr klub Turk",
        "unvon tilni kenja Volga karra mashq",
        "Aksariyat ishsizlik qoʻllanma Abdulloh",
        "tonna daryo Osiyo Pekin Suvda Yangi",
        "taxmin tasvir Gap tanqid Rasmiy Majlis",
        "rejimi Tadbir mumtoz noxush eng qarshi",
        "goʻsht ham kasbga Fermer Mehnat Adliya",
        "klassik boʻladi etilgan taomlar mavjud",
        "Maxsus yuqori Avvalo oʻz vazifa Gʻafur",
        "Demak Hozir suvni mavzu ashyo Nukus",
    ],
    &[
        "knyaz oʻyma oʻqqa ayvon birja sabzi",
        "Argentina gʻoyasini ularning Shvetsiya",
        "Navbahor Zarafshon e'tiborni gʻishtdan",
        "uning Oddiy yilda Qisqa linza Jomiy",
        "aqlli gripp qozoq Layli video Yusuf",
        "Ozarbayjon boshqa tomonidan maydonidir",
        "Ahmad Vatan Ayrim Temur qit'a oʻtga",
        "yil Misrda oldiga burgʻi Moskva orqali",
        "Albatta Rossiya hukmron muayyan Oʻzbek",
        "Katta Bemor hamda Islom bilan katta",
        "qatnovi iborat oktyabr ansambl Soʻnggi",
        "Olim Nerv Rim Bank vaqt Hind Umar Balx",
        "Kitob ta koʻra Paxta Hayot Dunyo Buyuk",
    ],
    &[
        "oʻtmishda Germaniya Institut ta'minlab",
        "ishga ushbu kimyo tarzi medal ya'ni",
        "asosiy xonligi neytral shaffof Sardoba",
        "muxbir me'yor Oʻquv muhim qismi paydo",
        "hosil qabul Lekin olish kishi yirik",
        "amalga Italiya bugʻdoy tashhis yumshoq",
        "Guliston qilingan bu inshooti biznesni",
        "Andijon uglerod davlat Yevropa Relyefi",
        "ezgu ijro Orqa dan Xalq Ammo ovul Xiva",
        "xoʻjalik Shunday boʻlgan ijtimoiy koʻp",
        "tuproqda she'rlar boʻlishi va Doʻstlik",
        "arpa sinf sarf Siz Ural koʻk Nima Agar",
        "tashkil notekis ishlab Qodiriy Korxona",
    ],
    &[
        "koreys esa qariyb qilgan Kurash Xorazm",
        "haykal xalqni jadval goh poetik maqbul",
        "Fan alp",
        "yangi shuningdek toʻplamlar antiseptik",
        "xavfli Afrika hujjat Aslida joizki har",
        "ekspertlar turli Toshkentda joylashgan",
        "tabiiy sferik ta'sir topgan rux tibbiy",
        "matbaa boʻlib mas'ul qiladi qilish bir",
        "qilib Respublika fiziologik Darhaqiqat",
        "suvga Nobel manba Diniy ustki Senat",
        "Muzey voqea fojia Jahon ilmiy kelsa",
        "sudning poyezd ogʻriq siljish Markaz",
        "biri ular Buni juda ming Eron igna Bir",
    ],
    &[
        "Gruziya Fransuz avtobus Saudiya beradi",
        "keng ikki Soʻx ish jagʻ Qayd zarb olib",
        "sulfat afgʻon mavsum podsho Ilk soʻmni",
        "kabi oid Quyi yoki xalq Boku jome zaif",
        "psixik deb mavqei uygʻur soʻmga raqsga",
        "Pop majbur oʻgʻli texnik Kavkaz tadqiq",
        "etsa xlor imzo aks Rus ibn xrom qasd",
        "roʻyxatga fakulteti jamoadagi quruqlik",
        "ga sifatida tezlikda tegishli Keyinroq",
        "Bakr sexi Film pivo Eng Neft fe'l a'zo",
        "Ha, toʻgʻri! Nega? Bu: \"ilm\" - kuch (aql).",
    ],
];

const EN_JOINED: &[&[&str]] = &[
    &[
        "underway new Edmonton Exchange innocent",
        "which their years can lucky could check",
        "Drive Floyd Vegas taxes Scott Given for",
        "Club just Unit your them what more year",
        "premium exposed testify Highway Michael",
        "would was Lives there songs write being",
        "Meanwhile I postponed practice achieved",
        "Emily Spain World Trade Apart who heavy",
        "when Blue Each Jeff news swim over camp",
        "on as we be at or in is he to an by it",
        "all its his she our him not her one arc",
        "last into than time this some they like",
        "City adhere object growth length outfit",
        "Museum during Cape Public Please Center",
    ],
    &[
        "Craig about worry Brown out after Watch",
        "Who God Air law gym Our Pro Cup Tim Act",
        "conscious occupied recommend calculated",
        "but are has you Luke had With Phil Bowl",
        "crucial unhappy section Stephen already",
        "other clubs now cycle Irish first equal",
        "lyrics Vice Office anyway August Europe",
        "crowds Atlanta two agency Health people",
        "were that Week also only from been Cook",
        "if my no so do up",
        "Fund Zoom easy type That Even debt York",
        "with caught always rescue Church degree",
        "solve input Once said will Ocean have",
        "Yes, right! Why? So: \"work\" - fun (yes).",
    ],
];

const RU_JOINED: &[&[&str]] = &[
    &[
        "обвиняют стрельбы психолог мы короткие",
        "коалиции экзамена области границу есть",
        "после показывать финансовых Информация",
        "Ночью Лучше не певец Общее Союза могут",
        "объясняет Владимир Оказалось поддержки",
        "Динамо длиной вообще всякий Второй них",
        "стерлингов спецслужбы время представил",
        "когда туалет тысяч можно Перед будут",
        "Германии об культуры смартфон Донецкой",
        "до групп услуг купил Врачи Ольга курсы",
        "мальчик Эстонии Турции техники Испании",
        "это том Джо Наш чём нет где дню она",
        "животными обсуждают Алтайском Например",
    ],
    &[
        "ребят во Омске долга рынке Земли клуба",
        "Вам Два Для При Его Они Сам мне Или",
        "птиц шоу Мой мэр очки Всё они Мне что",
        "снижается настоящий используя эпидемию",
        "Петербурге Татарстане Узбекистан очень",
        "было Лиги Ведь двух снят быть лет идёт",
        "перчатки Несмотря за учащихся Светлана",
        "церкви Сибири как Трампа звучит должны",
        "прошедшие пневмонии подсчетам Северной",
        "Полиция Конечно центров только Никитин",
        "возле вновь Очень месяц сумму бы Нужно",
        "всего Игорь Ирина то Пусть члены резко",
        "Байден тяжело отпуск эмоции под подход",
    ],
    &[
        "Именно звезды сердца взгляд сюжету все",
        "корпус вполне или Прежде съемки Жители",
        "Поэтому обычном Дмитрий субботу экипаж",
        "тайме полка бойцы обмен редко Будет",
        "неплохо Женщина больше Девушка Франции",
        "словам актриса Давайте формате антител",
        "самые Гарри от связи крыши людях Крыму",
        "закупки предмет Америки Евгений Каждый",
        "Дональд личных диагноз человек энергии",
        "Степанов внешнего колледжа Минздрав из",
        "офис врио цвет уйти сфер ищут уже Юлия",
        "были еще этот Иван Один того была знал",
        "Юрий без Днем лицо хуже язык знаю идеи",
    ],
    &[
        "пытался Россию которые чемпион Решение",
        "Люди при пять идти Если июля июня матч",
        "Штатов помощь говорю мнению Вместе был",
        "шаг бюро хлеб сбил даже если двум года",
        "Белоруссии находиться возглавлял может",
        "Елена фильм Думаю Ленте район её Павел",
        "Федерации хозяйств произошло появления",
        "Позже бренд ее Артем вызов Видео живет",
        "Запада января любому для штрафа Андрея",
        "футбол так Максим данные Пскове онлайн",
        "Благодаря сообщают аккуратно Известно",
        "начались на ощущение Карабаха субъекты",
        "Госдумы чувства выплату эксперт сейчас",
    ],
    &[
        "этом всех году жанра элиты уехал день",
        "средняя военную друзьям рублей включая",
        "заявил Впервые имеющие Главное октября",
        "чтобы дальнейшем заниматься масштабный",
        "окончил хватило выгодно который Работа",
        "Ещё лишь наук нашу флаг Банк Плюс свои",
        "Вопрос костюмы реакции Мужчина Украины",
        "трейлер Латвии Европы эффект Другие он",
        "выяснить аэропорт Особенно же мощности",
        "сцене их вчера Борис скажу храме входе",
        "раз оттуда выбора правда образе отдела",
        "солдаты Чувашии подписи Бывший ребёнка",
        "Это Ким зря юге",
    ],
    &[
        "Руслан его стыдно тройку выжить Грузии",
        "социальную этого популярных Отмечается",
        "будет уникальный украинских физической",
        "со Чехии книге белый Автор сырье также",
        "старше Японии Центра размер Италии чем",
        "Между цифры схеме улицы по семьи нефти",
        "но смысл вверх Кубка более мягко Центр",
        "Да, верно! Как? Так: «труд» - сила (ум).",
        "Йод Щит Ёж Эхо Юг Цех Жук Фея",
    ],
];

const UZ_CYRL_JOINED: &[&[&str]] = &[
    &[
        "хизматлар уйғунлиги жойлашган қуруқлик",
        "Маълумки касалхона хўжаликда топшириғи",
        "ташкил Шоҳ гуруҳи бўлган мавжуд Сунъий",
        "Тоғли Қўшма шу ишлаб сўриб ақлли арқоқ",
        "дўкони рақсга Фарҳод мавқеи Бағдод ҳам",
        "тахмин Қад Ҳусайн Ернинг омбори Тадбир",
        "Ижтимоий Жумладан ўз сайёҳлик уларнинг",
        "масжид Мажлис танқид Ўша масъул таҳсил",
        "пўлат юмшоқ топди ёд ёмғир Адлия Ишлаб",
        "Буюк яъни Филм феъл икки эга Орқа халқ",
        "олиш Юсуф Кўп Ўрта Хива бири жуда Янги",
        "оддий лимфа қабул Шунга яқини ўт Жомий",
    ],
    &[
        "сабзи Қуёш янги ўққа кўра Ҳаёт Халқ",
        "юқори маҳкум бўлди Шунда матбаа бошқа",
        "Эркин Аҳмад Ўзбек Айрим бу қалъа этган",
        "Шим Бир нақд деб энг Энг тоғ йўқ тахт",
        "Икки вақт Агар нашр аёл Асар Аммо жалб",
        "ноёб меҳр Ҳинд ижро ушбу фаол эди ўтди",
        "киши улар минг кенг каби ёруғ олиб рўй",
        "турли Ҳозир йирик қисми қилиб га қилиш",
        "тўғри унинг Ўтган ғалла Жаҳон ёғ муҳим",
        "афғон кенжа Ойбек айвон руҳда аҳвол",
        "манба Полша қатъи судга меъда Гуруҳ",
        "имтиёзли уюштирган раҳбарига томонидан",
    ],
    &[
        "Озарбайжон маъмурияти бўлиб Академияси",
        "та билан қарши Қисқа йилда этади сўмга",
        "нақш ёпиқ ошди кашф аъзо эски Сўз яшаш",
        "илмий Ғафур катта ҳамда ҳосил тўсиқ ва",
        "орқали қилган иборат қилади бўлиши ёки",
        "сиёсий ҳужжат тадқиқ йўл Жиззах Электр",
        "топган муайян таъсир жангга ҳар машҳур",
        "Фуқаролар шунингдек Абдуллоҳ мақбараси",
        "асосий кўп Лойиҳа амалга давлат мажмуи",
        "иншоот мухбир қизғин бир Қуръон халқни",
        "Улуғбек ҳисобга эҳтиёж қўнғир таъриф",
        "мумкин бўлади жўшқин эса ёшдаги асосан",
    ],
    &[
        "алп",
        "мўл умр Қон дуо гўё шўр Ибн Рим Ўзи",
        "япон забт хавф Ғарб сув нутқ Атом Кенг",
        "Афсуски этилган ёқилғи асосида эътибор",
        "иш Бразилия текшириш сифатида қилинган",
        "Термиз йил навбат сулфат Хоразм меҳнат",
        "Шайх Форс қарз уйга имзо эшик сарф бор",
        "қирғоқ ҳақиқат ўхшайди қўйишга Шаҳарда",
        "ҳукмдорлар майдонидир келинмоқда ҳуқуқ",
        "Ҳа, тўғри! Нега? Бу: «илм» - куч (ақл).",
    ],
];

// pages:end

const LATIN_SEPARATE: &[&str] = &[
    "a b d e f g h i j k l m n o p",
    "q r s t u v x y z o' g' c w",
    "A B D E F G H I J K L M N O P",
    "Q R S T U V X Y Z O' G' C W",
    "z y x v u t s r q p o n m l k",
    "j i h g f e d b a g' o' w c",
    "P O N M L K J I H G F E D B A",
    "W C G' O' Z Y X V U T S R Q",
    "0 1 2 3 4 5 6 7 8 9",
    "9 8 7 6 5 4 3 2 1 0",
    ". , : ; ! ? - ( ) \" ' %",
    "/ + = № ʻ ʼ",
    "2026 15 8:30 45 000 100% 12 + 35 = 47",
];

/// Russian and the Uzbek Cyrillic letters, written apart.
const CYRL_SEPARATE: &[&str] = &[
    "а б в г д е ё ж з и й к л м н о п",
    "р с т у ф х ц ч ш щ ъ ы ь э ю я",
    "А Б В Г Д Е Ё Ж З И Й К Л М Н О П",
    "Р С Т У Ф Х Ц Ч Ш Щ Ъ Ы Ь Э Ю Я",
    "я ю э ь ы ъ щ ш ч ц х ф у т с р",
    "п о н м л к й и з ж ё е д г в б а",
    "Я Ю Э Ь Ы Ъ Щ Ш Ч Ц Х Ф У Т С Р",
    "П О Н М Л К Й И З Ж Ё Е Д Г В Б А",
    "ў қ ғ ҳ Ў Қ Ғ Ҳ ҳ ғ қ ў Ҳ Ғ Қ Ў",
    "0 1 2 3 4 5 6 7 8 9",
    ". , : ; ! ? - ( ) \" ' %",
    "/ + = № « » —",
];

const MATH: &[&str] = &[
    "2 + 3 = 5   7 - 4 = 3   6 × 8 = 48",
    "81 ÷ 9 = 9   √49 = 7   π ≈ 3,14",
    "x + y = 10   x - y = 2   x = 6",
    "a ≤ b   c ≥ d   m ≠ n   0 < k < 5",
    "1/2 + 1/4 = 3/4   0,5 + 0,25 = 0,75",
    "±3   90°   α + β = 180°",
    "f(x) = 3x + 1   f(2) = 7",
    "S = a · b   P = 2(a + b)",
    "[1; 4]   {2, 3}   ∑ = 10",
    "Δ = 16 - 12 = 4   t → ∞",
    "∫   θ   λ   μ   σ   φ   ω   γ",
];

/// Uzbek Latin to Uzbek Cyrillic.
pub fn to_cyrillic(text: &str) -> String {
    let t = charset::normalize(text);
    let cs: Vec<char> = t.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    let word_start = |i: usize| i == 0 || !cs[i - 1].is_alphabetic();
    while i < cs.len() {
        let c = cs[i];
        let n = cs.get(i + 1).copied();
        let up = c.is_uppercase();
        let lower = c.to_lowercase().next().unwrap_or(c);
        let pair = n.map(|n| (lower, n.to_lowercase().next().unwrap_or(n)));
        let two = match pair {
            Some(('s', 'h')) => Some("ш"),
            Some(('c', 'h')) => Some("ч"),
            Some(('g', 'ʻ')) => Some("ғ"),
            Some(('o', 'ʻ')) => Some("ў"),
            // yoʻ is й + ў, not ё
            Some(('y', 'o')) if cs.get(i + 2) == Some(&'ʻ') => Some("й"),
            Some(('y', 'o')) => Some("ё"),
            Some(('y', 'u')) => Some("ю"),
            Some(('y', 'a')) => Some("я"),
            Some(('y', 'e')) if word_start(i) => Some("е"),
            _ => None,
        };
        let (s, step) = match two {
            Some("й") => ("й".to_string(), 1),
            Some(s) => (s.to_string(), 2),
            None => {
                let one = match lower {
                    'a' => "а", 'b' => "б", 'd' => "д", 'e' => if word_start(i) { "э" } else { "е" }, 'f' => "ф",
                    'g' => "г", 'h' => "ҳ", 'i' => "и", 'j' => "ж", 'k' => "к", 'l' => "л", 'm' => "м",
                    'n' => "н", 'o' => "о", 'p' => "п", 'q' => "қ", 'r' => "р", 's' => "с", 't' => "т",
                    'u' => "у", 'v' => "в", 'x' => "х", 'y' => "й", 'z' => "з", '\'' => "ъ",
                    // Cyrillic text uses «guillemets»
                    '"' => if i == 0 || cs[i - 1] == ' ' || cs[i - 1] == '(' { "«" } else { "»" },
                    _ => "",
                };
                (if one.is_empty() { c.to_string() } else { one.to_string() }, 1)
            }
        };
        if up {
            let mut it = s.chars();
            if let Some(f) = it.next() {
                out.extend(f.to_uppercase());
                out.push_str(it.as_str());
            }
        } else {
            out.push_str(&s);
        }
        i += step;
    }
    out
}

#[derive(Serialize, Clone, Debug)]
pub struct Page {
    /// "uz_latn-1", "uz_latn-s" (separate characters), "math-1", "extra-N"
    pub id: String,
    pub lang: Option<Lang>,
    pub kind: Kind,
    /// number among the pages of its kind and language
    pub n: usize,
    pub lines: Vec<String>,
    /// per line: written in print letters (letters apart, like a printed T)
    pub print: Vec<bool>,
}

fn norm(p: &[&str]) -> Vec<String> {
    p.iter().map(|s| charset::normalize(s)).collect()
}

/// The built-in languages, the person's main one first.
pub fn order(main: Lang) -> Vec<Lang> {
    let mut v = vec![main];
    v.extend([Lang::UzLatn, Lang::Ru, Lang::UzCyrl, Lang::En, Lang::Math].into_iter().filter(|l| *l != main));
    v
}

/// A language's pages: joined writing first, then characters written apart. English and
/// Uzbek Cyrillic have the same letters as Uzbek Latin and Russian, whose separate pages
/// already hold them.
pub fn lang_pages(l: Lang) -> Vec<Page> {
    let (joined, separate): (&[&[&str]], Option<&[&str]>) = match l {
        Lang::UzLatn => (UZ_LATN_JOINED, Some(LATIN_SEPARATE)),
        Lang::En => (EN_JOINED, None),
        Lang::Ru => (RU_JOINED, Some(CYRL_SEPARATE)),
        Lang::UzCyrl => (UZ_CYRL_JOINED, None),
        Lang::Math => return vec![Page { id: "math-1".into(), lang: Some(l), kind: Kind::Math, n: 1, print: vec![false; MATH.len()], lines: norm(MATH) }],
    };
    let mut out: Vec<Page> = joined
        .iter()
        .enumerate()
        .map(|(i, lines)| Page { id: format!("{}-{}", l.key(), i + 1), lang: Some(l), kind: Kind::Joined, n: i + 1, print: vec![false; lines.len()], lines: norm(lines) })
        .collect();
    // most people print English: its last page is written in print letters
    if l == Lang::En {
        if let Some(p) = out.last_mut() {
            p.kind = Kind::Print;
            p.n = 1;
            p.print = vec![true; p.lines.len()];
        }
    }
    // the characters apart: print letters (shaped like a printed T), digits and marks
    if let Some(lines) = separate {
        out.push(Page { id: format!("{}-s", l.key()), lang: Some(l), kind: Kind::Separate, n: 1, print: vec![true; lines.len()], lines: norm(lines) });
    }
    out
}

/// Pages of only the characters not written yet, each three times apart: letters as the
/// person writes them, then print letters (lines marked print), digits and marks, maths.
/// Numbered from `first`.
pub fn fill_pages(hand: &Hand, first: usize) -> Vec<Page> {
    let cov = hand.coverage();
    let mut lines: Vec<(String, bool)> = vec![];
    for key in ["latin", "cyrillic", "latin_print", "cyrillic_print", "digits", "math"] {
        let Some(g) = cov.iter().find(|g| g.group == key) else { continue };
        let print = key.ends_with("_print");
        let mut cur = String::new();
        for c in g.chars.iter().filter(|c| c.n == 0).map(|c| c.c) {
            let token = format!("{c} {c} {c}");
            if !cur.is_empty() && cur.chars().count() + 3 + token.chars().count() > 34 {
                lines.push((std::mem::take(&mut cur), print));
            }
            if !cur.is_empty() {
                cur.push_str("   ");
            }
            cur.push_str(&token);
        }
        if !cur.is_empty() {
            lines.push((cur, print));
        }
    }
    lines
        .chunks(14)
        .enumerate()
        .map(|(i, ch)| Page {
            id: format!("fill-{}", first + i),
            lang: None,
            kind: Kind::Fill,
            n: first + i,
            lines: ch.iter().map(|l| l.0.clone()).collect(),
            print: ch.iter().map(|l| l.1).collect(),
        })
        .collect()
}

/// Every built-in page by language, the main one first: all of them are needed to start.
/// English is written with the Latin letters of Uzbek, so its pages are not asked for (unless
/// it is the person's main language).
pub fn required(main: Lang) -> Vec<(Lang, Vec<Page>)> {
    order(main).into_iter().filter(|l| *l != Lang::En || main == Lang::En).map(|l| (l, lang_pages(l))).collect()
}

/// The words of a line, without the punctuation around them.
fn words(line: &str) -> impl Iterator<Item = Vec<char>> + '_ {
    line.split_whitespace()
        .map(|w| w.trim_matches(|c: char| ",.;:!?\"()«»-—".contains(c)).chars().collect::<Vec<char>>())
        .filter(|w| !w.is_empty())
}

/// What the written pages gave: characters, and letter pairs that stood one after the
/// other in a word (the joins the writer can put back exactly as written).
pub fn written(hand: &Hand) -> (HashSet<char>, HashSet<(char, char)>) {
    let mut chars = HashSet::new();
    let mut by_word: HashMap<u64, Vec<(u16, &str)>> = HashMap::new();
    // joined writing only: print letters are asked for on pages of their own
    for g in hand.glyphs.iter().filter(|g| g.on && g.src != Src::Auto && !g.print) {
        chars.extend(g.text.chars());
        if let Some(x) = &g.ctx {
            by_word.entry(x.word).or_default().push((x.pos, &g.text));
        }
    }
    let mut pairs = HashSet::new();
    for gs in by_word.values_mut() {
        gs.sort();
        for w in gs.windows(2) {
            let ((pa, a), (pb, b)) = (w[0], w[1]);
            if pb as usize == pa as usize + a.chars().count() {
                if let (Some(x), Some(y)) = (a.chars().last(), b.chars().next()) {
                    pairs.insert((x, y));
                }
            }
        }
        for (_, t) in gs.iter() {
            let cs: Vec<char> = t.chars().collect();
            pairs.extend(cs.windows(2).map(|p| (p[0], p[1])));
        }
    }
    (chars, pairs)
}

/// A page of the words whose letters or joins did not come through (a word was not found,
/// a letter was cut badly), taken from the required pages; a character no joined word holds
/// is written on its own.
pub fn missing_page(hand: &Hand, n: usize) -> Option<Page> {
    let (chars, pairs) = written(hand);
    let have_char = |c: char| charset::same(c).into_iter().any(|x| chars.contains(&x));
    let all: Vec<Page> = Lang::ALL.into_iter().flat_map(lang_pages).collect();
    let mut pool: Vec<Vec<char>> = vec![];
    let mut seen = HashSet::new();
    for p in all.iter().filter(|p| matches!(p.kind, Kind::Joined | Kind::Print)) {
        for l in &p.lines {
            for w in words(l) {
                if seen.insert(w.clone()) {
                    pool.push(w);
                }
            }
        }
    }
    let mut need_c: Vec<char> = vec![];
    for p in &all {
        for c in p.lines.concat().chars() {
            if !c.is_whitespace() && !have_char(c) && !need_c.contains(&c) {
                need_c.push(c);
            }
        }
    }
    let mut need_p: HashSet<(char, char)> = pool.iter().flat_map(|w| w.windows(2).map(|p| (p[0], p[1])).collect::<Vec<_>>()).filter(|p| !pairs.contains(p)).collect();
    if need_c.is_empty() && need_p.is_empty() {
        return None;
    }
    let mut tokens: Vec<String> = vec![];
    // the words that hold the most of what is missing, for the letters they cost
    loop {
        let gain = |w: &Vec<char>| -> f32 {
            let p = w.windows(2).filter(|p| need_p.contains(&(p[0], p[1]))).count();
            let c = w.iter().filter(|c| need_c.contains(c)).count();
            (p + 2 * c) as f32 / (w.len() + 1) as f32
        };
        let Some((i, g)) = pool.iter().enumerate().map(|(i, w)| (i, gain(w))).max_by(|a, b| a.1.partial_cmp(&b.1).unwrap()) else { break };
        if g <= 0.0 || tokens.len() >= 70 {
            break;
        }
        let w = pool.swap_remove(i);
        for p in w.windows(2) {
            need_p.remove(&(p[0], p[1]));
        }
        need_c.retain(|c| !w.contains(c));
        tokens.push(w.into_iter().collect());
    }
    for c in need_c {
        tokens.extend([c.to_string(), c.to_string()]);
    }
    let mut lines: Vec<String> = vec![];
    let mut cur = String::new();
    for t in tokens {
        if !cur.is_empty() && cur.chars().count() + 1 + t.chars().count() > 38 {
            lines.push(std::mem::take(&mut cur));
        }
        if !cur.is_empty() {
            cur.push(' ');
        }
        cur.push_str(&t);
    }
    if !cur.is_empty() {
        lines.push(cur);
    }
    lines.truncate(14);
    Some(Page { id: format!("extra-{n}"), lang: None, kind: Kind::Extra, n, print: vec![false; lines.len()], lines })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn script(l: Lang) -> [Lang; 2] {
        if l.latin() { [Lang::UzLatn, Lang::En] } else { [Lang::Ru, Lang::UzCyrl] }
    }

    /// How often each character appears on the pages of a script, of one kind.
    fn counts(l: Lang, kind: Kind) -> HashMap<char, usize> {
        let mut m = HashMap::new();
        for s in script(l) {
            for p in lang_pages(s).iter().filter(|p| p.kind == kind) {
                for c in p.lines.concat().chars() {
                    *m.entry(c).or_insert(0) += 1;
                }
            }
        }
        m
    }

    #[test]
    fn pages_cover_every_character() {
        let mut missing = vec![];
        for l in [Lang::UzLatn, Lang::UzCyrl, Lang::Ru, Lang::En] {
            let joined = counts(l, Kind::Joined);
            let separate = counts(l, Kind::Separate);
            for (group, chars) in charset::target_chars(&[l]) {
                for c in chars {
                    // every character written apart at least once
                    if !separate.contains_key(&c) {
                        missing.push(format!("{l:?} separate {c}"));
                    }
                    // small letters in many joins; capitals at the start of words
                    if group != "digits" {
                        let rare = "ЪЫЬъЁ".contains(c);
                        let want = if c.is_lowercase() { 6 } else { 1 };
                        if !rare && joined.get(&c).copied().unwrap_or(0) < want {
                            missing.push(format!("{l:?} joined {c} ({})", joined.get(&c).copied().unwrap_or(0)));
                        }
                    }
                }
            }
            for p in lang_pages(l) {
                assert!(p.lines.len() <= 14, "{}", p.id);
                for line in &p.lines {
                    assert!(line.chars().count() <= 42, "{line}");
                }
            }
        }
        assert!(missing.is_empty(), "pages miss: {missing:?}");
        assert_eq!(to_cyrillic("Bugun ertalab yaxshi edi, o'zbek shahar Yusuf"), "Бугун эрталаб яхши эди, ўзбек шаҳар Юсуф");
        assert_eq!(to_cyrillic("Yo'lda yozda bog'"), "Йўлда ёзда боғ");
        assert_eq!(required(Lang::Ru)[0].0, Lang::Ru);
        assert_eq!(required(Lang::UzLatn).len(), 4);
        assert!(required(Lang::UzLatn).iter().all(|(l, _)| *l != Lang::En));
        let en = lang_pages(Lang::En);
        assert_eq!((en[0].kind, en[1].kind), (Kind::Joined, Kind::Print));
    }

    /// The joins of real text (pair counts from news and encyclopedia text, 100K sentences a
    /// language): nearly all of them are on the pages of their script, most of them twice.
    #[test]
    fn pages_hold_the_joins_of_real_text() {
        let tsv = include_str!("../tests/data/pairs.tsv");
        for l in [Lang::UzLatn, Lang::En, Lang::Ru, Lang::UzCyrl] {
            let mut have: HashMap<(char, char), usize> = HashMap::new();
            for s in script(l) {
                for p in lang_pages(s).iter().filter(|p| matches!(p.kind, Kind::Joined | Kind::Print)) {
                    for line in &p.lines {
                        for w in words(line) {
                            for p in w.windows(2) {
                                *have.entry((p[0], p[1])).or_insert(0) += 1;
                            }
                        }
                    }
                }
            }
            let (mut tot, mut one, mut two) = (0u64, 0u64, 0u64);
            for row in tsv.lines() {
                let mut f = row.split('\t');
                let (Some(k), Some(pair), Some(n)) = (f.next(), f.next(), f.next()) else { continue };
                if k != l.key() {
                    continue;
                }
                let cs: Vec<char> = pair.chars().collect();
                let n: u64 = n.parse().unwrap();
                let h = have.get(&(cs[0], cs[1])).copied().unwrap_or(0);
                tot += n;
                one += n * (h >= 1) as u64;
                two += n * (h >= 2) as u64;
            }
            let (one, two) = (one as f64 / tot as f64, two as f64 / tot as f64);
            assert!(one >= 0.985 && two >= 0.88, "{l:?}: joins on the pages {one:.3}, twice {two:.3}");
        }
    }

    #[test]
    fn missing_page_asks_again() {
        let h = Hand::new(crate::hand::HandMeta::new("t", "t"), vec![], std::path::PathBuf::new());
        // nothing written yet: every page's character is missing, so the page asks for words
        let p = missing_page(&h, 1).unwrap();
        assert_eq!(p.kind, Kind::Extra);
        assert!(!p.lines.is_empty() && p.lines.len() <= 14);
    }

    #[test]
    fn fill_pages_hold_only_what_is_missing() {
        let mut meta = crate::hand::HandMeta::new("t", "t");
        meta.langs = vec![Lang::UzLatn];
        let mut h = Hand::new(meta, vec![], std::path::PathBuf::new());
        let ps = fill_pages(&h, 3);
        assert_eq!(ps[0].id, "fill-3");
        // letters as the person writes them come first, then the print ones
        let all: Vec<(String, bool)> = ps.iter().flat_map(|p| p.lines.iter().cloned().zip(p.print.iter().copied())).collect();
        assert!(all[0].0.starts_with("a a a") && !all[0].1);
        let first_print = all.iter().position(|l| l.1).unwrap();
        assert!(all[first_print].0.starts_with("a a a"));
        assert!(ps.iter().all(|p| p.lines.len() <= 14 && p.lines.len() == p.print.len()));
        // a joined a written: only the print one is asked for
        let id = "x".to_string();
        h.glyphs.push(serde_json::from_value(serde_json::json!({"id": id, "text": "a", "src": "text", "w": 1, "h": 1, "bl": 1.0, "ppm": 10.0, "on": true, "sw": 1.0, "skel": [], "entry": null, "exit": null, "ink_x": [0.0, 1.0]})).unwrap());
        let all: Vec<(String, bool)> = fill_pages(&h, 1).iter().flat_map(|p| p.lines.iter().cloned().zip(p.print.iter().copied())).collect();
        assert!(all[0].0.starts_with("b b b"));
        assert!(all.iter().find(|l| l.1).unwrap().0.starts_with("a a a"));
    }
}
