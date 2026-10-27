use crate::tr;
use crate::window_position::{PopupCorner, SavedPopupPosition};
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};

pub const MAX_PACK_BYTES: u64 = 2 * 1024 * 1024;
pub const MAX_STATE_BYTES: u64 = 16 * 1024 * 1024;

// Use one catalog for both fresh content and its initial participation settings.
const STARTER_PACKS: &[(&str, &[u8])] = &[
    (
        "math_recall",
        include_bytes!("../knowledge_packs/math_recall.json"),
    ),
    (
        "english_recall",
        include_bytes!("../knowledge_packs/english_recall.json"),
    ),
    (
        "programming_recall",
        include_bytes!("../knowledge_packs/programming_recall.json"),
    ),
    (
        "chemistry_recall",
        include_bytes!("../knowledge_packs/chemistry_recall.json"),
    ),
    (
        "general_recall",
        include_bytes!("../knowledge_packs/general_recall.json"),
    ),
];

pub type AppResult<T> = Result<T, String>;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnswerMode {
    #[default]
    Input,
    MultipleChoice,
    TrueFalse,
}

impl AnswerMode {
    pub const ALL: [Self; 3] = [Self::Input, Self::MultipleChoice, Self::TrueFalse];

    pub fn label(self) -> &'static str {
        match self {
            Self::Input => tr!("填空题"),
            Self::MultipleChoice => tr!("选择题"),
            Self::TrueFalse => tr!("判断题"),
        }
    }

    pub fn index(self) -> i32 {
        match self {
            Self::Input => 0,
            Self::MultipleChoice => 1,
            Self::TrueFalse => 2,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PracticeConfig {
    #[default]
    Custom,
    TrueFalse,
    MultipleChoice,
    Input,
}

impl PracticeConfig {
    pub fn answer_mode(self) -> Option<AnswerMode> {
        match self {
            Self::Custom => None,
            Self::TrueFalse => Some(AnswerMode::TrueFalse),
            Self::MultipleChoice => Some(AnswerMode::MultipleChoice),
            Self::Input => Some(AnswerMode::Input),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnswerContent {
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latex: Option<String>,
}

impl AnswerContent {
    /// Compare declared spellings and the displayed formula without evaluating
    /// algebra or numerical approximations. Plain fallback text alone can differ.
    fn same_notation(&self, text: &str, latex: Option<&str>) -> bool {
        self.text.trim() == text.trim()
            || crate::math_answer::matches(
                self.latex.as_deref().unwrap_or(&self.text),
                text,
                latex,
                &[],
            )
            || crate::math_answer::matches(
                latex.unwrap_or(text),
                &self.text,
                self.latex.as_deref(),
                &[],
            )
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrueFalse {
    pub statement: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub statement_latex: Option<String>,
    pub is_true: bool,
    pub correction: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub correction_latex: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Card {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_image: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub prompt: String,
    pub answer: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_latex: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub answer_latex: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub aliases: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub distractors: Vec<AnswerContent>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub true_false: Option<TrueFalse>,
}

impl Card {
    pub fn supports(&self, mode: AnswerMode) -> bool {
        match mode {
            AnswerMode::Input => true,
            AnswerMode::MultipleChoice => self.distractors.len() == 2,
            AnswerMode::TrueFalse => self.true_false.is_some(),
        }
    }

    pub fn accepts(&self, input: &str) -> bool {
        let input = input.trim();
        !input.is_empty()
            && (input == self.answer.trim()
                || self.aliases.iter().any(|alias| input == alias.trim())
                || crate::math_answer::matches(
                    input,
                    &self.answer,
                    self.answer_latex.as_deref(),
                    &self.aliases,
                ))
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KnowledgePack {
    pub format: String,
    pub schema_version: u32,
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub tag: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    pub cards: Vec<Card>,
}

impl KnowledgePack {
    // Legacy packs have no card identifiers. Match unchanged content when
    // replacing one, rather than treating its current row number as identity.
    fn assign_card_ids(&mut self, previous: Option<&Self>) -> bool {
        if self.cards.iter().all(|card| card.id.is_some()) {
            return false;
        }
        let mut used: HashSet<String> = self.cards.iter().filter_map(|c| c.id.clone()).collect();
        let mut old: HashMap<String, VecDeque<String>> = HashMap::new();
        if let Some(previous) = previous {
            for card in &previous.cards {
                if let Some(id) = &card.id {
                    old.entry(card_content_key(card))
                        .or_default()
                        .push_back(id.clone());
                }
            }
        }
        for card in &mut self.cards {
            if card.id.is_some() {
                continue;
            }
            let retained = old.get_mut(&card_content_key(card)).and_then(|ids| {
                while let Some(id) = ids.pop_front() {
                    if used.insert(id.clone()) {
                        return Some(id);
                    }
                }
                None
            });
            card.id = Some(retained.unwrap_or_else(|| {
                loop {
                    let id = format!("card_{:032x}", rand::rng().random::<u128>());
                    if used.insert(id.clone()) {
                        break id;
                    }
                }
            }));
        }
        true
    }

    pub fn supports(&self, mode: AnswerMode) -> bool {
        !self.cards.is_empty() && self.cards.iter().all(|card| card.supports(mode))
    }

    pub fn available_modes(&self) -> Vec<AnswerMode> {
        AnswerMode::ALL
            .into_iter()
            .filter(|mode| self.supports(*mode))
            .collect()
    }

    pub fn from_json(bytes: &[u8]) -> AppResult<Self> {
        if bytes.len() as u64 > MAX_PACK_BYTES {
            return Err(tr!("知识包超过 2 MiB，请拆成几个小文件。").into());
        }
        // Accept UTF-8 files saved by editors that include a byte-order mark.
        let bytes = bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(bytes);
        let pack: Self = serde_json::from_slice(bytes)
            .map_err(|error| tr!("无法读取知识包 JSON：{error}", error = error))?;
        pack.validate()?;
        Ok(pack)
    }

    pub fn validate(&self) -> AppResult<()> {
        if self.format != "try" || self.schema_version != 1 {
            return Err(tr!("请选择 format 为 try、schema_version 为 1 的知识包。").into());
        }
        if self.id.is_empty()
            || self.id.len() > 80
            || !self
                .id
                .bytes()
                .next()
                .is_some_and(|c| c.is_ascii_lowercase())
            || !self
                .id
                .bytes()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'_')
        {
            return Err(
                tr!("知识包 id 须以小写英文字母开头，只使用小写字母、数字和下划线。").into(),
            );
        }
        if self.name.trim().is_empty() || self.name.chars().count() > 80 {
            return Err(tr!("知识包名称须为 1 至 80 个字符。").into());
        }
        validate_pack_tag(&self.tag)?;
        if self.description.chars().count() > 2000 {
            return Err(tr!("知识包介绍最多 2000 个字符。").into());
        }
        if self.cards.len() > 2000 {
            return Err(tr!("每个知识包最多保存 2000 道题。").into());
        }
        let mut card_ids = HashSet::new();
        for (index, card) in self.cards.iter().enumerate() {
            if card
                .prompt_image
                .as_ref()
                .is_some_and(|id| !crate::media::valid_id(id))
            {
                return Err(tr!("图片资源标识无效。").into());
            }
            if let Some(id) = &card.id
                && (!valid_id(id) || !card_ids.insert(id))
            {
                return Err(tr!("第 {p0} 题的 id 无效或重复。", p0 = index + 1));
            }
            for (field, value, allow_empty) in [
                (tr!("题面"), &card.prompt, card.prompt_image.is_some()),
                (tr!("答案"), &card.answer, false),
            ] {
                if (value.trim().is_empty() && !allow_empty) || value.chars().count() > 1000 {
                    return Err(tr!(
                        "第 {p0} 题的{field}须为 1 至 1000 个字符。",
                        p0 = index + 1,
                        field = field
                    ));
                }
            }
            for (field, value) in [
                (tr!("题面"), &card.prompt_latex),
                (tr!("答案"), &card.answer_latex),
            ] {
                if let Some(tex) = value {
                    crate::formula::validate(tex).map_err(|error| {
                        tr!(
                            "第 {p0} 题的{field}{error}",
                            p0 = index + 1,
                            field = field,
                            error = error
                        )
                    })?;
                }
            }
            if card.aliases.len() > 32
                || card
                    .aliases
                    .iter()
                    .any(|alias| alias.trim().is_empty() || alias.chars().count() > 1000)
            {
                return Err(tr!("第 {p0} 题的答案别名为空或过长。", p0 = index + 1));
            }
            let validate_content = |text: &str, latex: &Option<String>| {
                if text.trim().is_empty() || text.chars().count() > 1000 {
                    return Err(tr!(
                        "第 {p0} 题的选项或判断陈述须为 1 至 1000 个字符。",
                        p0 = index + 1
                    ));
                }
                if let Some(tex) = latex {
                    crate::formula::validate(tex)
                        .map_err(|error| tr!("第 {p0} 题{error}", p0 = index + 1, error = error))?;
                }
                Ok(())
            };
            if !card.distractors.is_empty() {
                if card.distractors.len() != 2 {
                    return Err(tr!("第 {p0} 题需要两个干扰项。", p0 = index + 1));
                }
                let mut distinct: Vec<&AnswerContent> = Vec::new();
                for option in &card.distractors {
                    validate_content(&option.text, &option.latex)?;
                    if option.same_notation(&card.answer, card.answer_latex.as_deref())
                        || card
                            .aliases
                            .iter()
                            .any(|alias| option.same_notation(alias, None))
                        || distinct.iter().any(|previous| {
                            option.same_notation(&previous.text, previous.latex.as_deref())
                        })
                    {
                        return Err(tr!(
                            "第 {p0} 题的干扰项重复，或与正确答案相同。",
                            p0 = index + 1
                        ));
                    }
                    distinct.push(option);
                }
            }
            if let Some(statement) = &card.true_false {
                validate_content(&statement.statement, &statement.statement_latex)?;
                validate_content(&statement.correction, &statement.correction_latex)?;
            }
        }
        Ok(())
    }
}

fn validate_pack_tag(tag: &str) -> AppResult<()> {
    if tag.chars().count() > 24
        || tag
            .chars()
            .any(|ch| ch.is_control() || matches!(ch, '\u{2028}' | '\u{2029}'))
    {
        return Err(tr!("知识包标签最多 24 个字符，不能换行；留空则不显示。").into());
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Appearance {
    Light,
    Dark,
    #[default]
    System,
}

impl Appearance {
    pub fn index(self) -> i32 {
        match self {
            Self::Light => 0,
            Self::Dark => 1,
            Self::System => 2,
        }
    }

    pub fn from_index(index: i32) -> Option<Self> {
        match index {
            0 => Some(Self::Light),
            1 => Some(Self::Dark),
            2 => Some(Self::System),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
    pub appearance: Appearance,
    pub interface_language: crate::i18n::InterfaceLanguage,
    // Read the old total switch once; a disabled switch becomes UntilResumed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reminders_enabled: Option<bool>,
    pub pause: crate::reminder_schedule::PracticePause,
    pub interval_minutes: u32,
    pub feedback_seconds: u32,
    pub knowledge_seconds: u32,
    pub knowledge_only: bool,
    pub focus_on_reminder: bool,
    pub auto_check_updates: bool,
    pub quiet_hours: crate::reminder_schedule::QuietHours,
    pub enabled_pack_ids: Vec<String>,
    pub popup_corner: PopupCorner,
    pub show_pack_tags: bool,
    pub remember_position: bool,
    pub popup_position: Option<SavedPopupPosition>,
    // Read legacy pack-wide preferences, then migrate them into question settings.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub answer_modes: BTreeMap<String, AnswerMode>,
    pub card_answer_modes: BTreeMap<String, BTreeMap<String, AnswerMode>>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub pack_practice_configs: BTreeMap<String, PracticeConfig>,
    // A disabled pack retains its question range for the next explicit activation.
    pub excluded_card_ids: BTreeMap<String, BTreeSet<String>>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            appearance: Appearance::default(),
            interface_language: crate::i18n::InterfaceLanguage::default(),
            reminders_enabled: None,
            pause: crate::reminder_schedule::PracticePause::None,
            interval_minutes: 30,
            feedback_seconds: 8,
            knowledge_seconds: 8,
            knowledge_only: false,
            focus_on_reminder: true,
            auto_check_updates: true,
            quiet_hours: crate::reminder_schedule::QuietHours::default(),
            enabled_pack_ids: STARTER_PACKS.iter().map(|(id, _)| (*id).into()).collect(),
            popup_corner: PopupCorner::BottomRight,
            show_pack_tags: true,
            remember_position: true,
            popup_position: None,
            answer_modes: BTreeMap::new(),
            card_answer_modes: BTreeMap::new(),
            pack_practice_configs: BTreeMap::new(),
            excluded_card_ids: BTreeMap::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Library {
    pub format: String,
    pub schema_version: u32,
    #[serde(deserialize_with = "deserialize_profile_settings")]
    pub settings: Settings,
    pub packs: Vec<KnowledgePack>,
}

fn deserialize_profile_settings<'de, D>(deserializer: D) -> Result<Settings, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let mut value = serde_json::Value::deserialize(deserializer)?;
    // Older profiles shared one duration. Preserve it for both uses on load;
    // subsequent changes save the two independent values without a startup write.
    if let Some(fields) = value.as_object_mut()
        && !fields.contains_key("knowledge_seconds")
        && let Some(feedback) = fields.get("feedback_seconds").cloned()
    {
        fields.insert("knowledge_seconds".into(), feedback);
    }
    Settings::deserialize(value).map_err(serde::de::Error::custom)
}

impl Default for Library {
    fn default() -> Self {
        let mut library = Self {
            format: "try_state".into(),
            schema_version: 1,
            settings: Settings::default(),
            packs: STARTER_PACKS
                .iter()
                .map(|(_, bytes)| {
                    KnowledgePack::from_json(bytes).expect("bundled starter packs must be valid")
                })
                .collect(),
        };
        library.ensure_card_ids();
        library
    }
}

impl Library {
    pub fn ensure_card_ids(&mut self) -> bool {
        let mut changed = false;
        for pack in &mut self.packs {
            changed |= pack.assign_card_ids(None);
        }
        changed
    }

    pub fn migrate_legacy_answer_modes(&mut self) -> bool {
        if self.settings.answer_modes.is_empty() {
            return false;
        }
        for (pack_id, mode) in std::mem::take(&mut self.settings.answer_modes) {
            if mode == AnswerMode::Input {
                continue;
            }
            if let Some(pack) = self.packs.iter().find(|pack| pack.id == pack_id) {
                let modes = self.settings.card_answer_modes.entry(pack_id).or_default();
                for card in &pack.cards {
                    if let Some(id) = &card.id {
                        modes.entry(id.clone()).or_insert(mode);
                    }
                }
            }
        }
        true
    }

    /// Flatten the former persistent override into the modes the user actually saw.
    /// Taking the map makes migration idempotent, including for an all-input pack.
    pub fn migrate_legacy_practice_configs(&mut self) -> bool {
        if self.settings.pack_practice_configs.is_empty() {
            return false;
        }
        for (pack_id, config) in std::mem::take(&mut self.settings.pack_practice_configs) {
            if let Some(mode) = config.answer_mode() {
                self.set_all_answer_modes(&pack_id, mode)
                    .expect("legacy settings are validated before migration");
            }
        }
        true
    }

    pub fn validate(&self) -> AppResult<()> {
        if self.format != "try_state" || self.schema_version != 1 {
            return Err(tr!("本地数据版本暂不支持，请保留原文件。").into());
        }
        self.settings.quiet_hours.validate()?;
        self.settings.pause.validate()?;
        if !(1..=1440).contains(&self.settings.interval_minutes)
            || !(1..=60).contains(&self.settings.feedback_seconds)
            || !(1..=60).contains(&self.settings.knowledge_seconds)
        {
            return Err(tr!("出题间隔须为 1 至 1440 分钟，两项停留时间须为 1 至 60 秒。").into());
        }
        if self.packs.len() > 64
            || self
                .packs
                .iter()
                .map(|pack| pack.cards.len())
                .sum::<usize>()
                > 20_000
        {
            return Err(tr!("首版最多保存 64 个知识包、共 20000 道题。").into());
        }
        if self
            .settings
            .popup_position
            .as_ref()
            .is_some_and(|position| !position.is_valid())
        {
            return Err(tr!("保存的练习窗口位置无效，请保留原文件。").into());
        }
        let mut ids = HashSet::new();
        for pack in &self.packs {
            pack.validate()?;
            if !ids.insert(pack.id.as_str()) {
                return Err(tr!("知识包 id 重复：{p0}", p0 = pack.id));
            }
        }
        let mut enabled = HashSet::new();
        for id in &self.settings.enabled_pack_ids {
            if !ids.contains(id.as_str()) || !enabled.insert(id.as_str()) {
                return Err(tr!("启用的知识包列表含有不存在或重复的 id。").into());
            }
        }
        for (id, mode) in &self.settings.answer_modes {
            if !self
                .packs
                .iter()
                .any(|pack| &pack.id == id && pack.supports(*mode))
            {
                return Err(tr!("答题方式引用了不存在或内容不完整的知识包。").into());
            }
        }
        for (pack_id, modes) in &self.settings.card_answer_modes {
            let pack = self
                .packs
                .iter()
                .find(|pack| &pack.id == pack_id)
                .ok_or(tr!("答题方式引用了不存在的知识包。"))?;
            for (card_id, mode) in modes {
                if !pack.cards.iter().any(|card| {
                    card.id.as_deref() == Some(card_id.as_str()) && card.supports(*mode)
                }) {
                    return Err(tr!("答题方式引用了不存在或内容不完整的题目。").into());
                }
            }
        }
        for (pack_id, config) in &self.settings.pack_practice_configs {
            if !self.packs.iter().any(|pack| {
                &pack.id == pack_id && config.answer_mode().is_none_or(|mode| pack.supports(mode))
            }) {
                return Err(tr!("练习配置引用了不存在或内容不完整的知识包。").into());
            }
        }
        for (pack_id, excluded) in &self.settings.excluded_card_ids {
            let pack = self
                .packs
                .iter()
                .find(|pack| &pack.id == pack_id)
                .ok_or(tr!("单题参与设置引用了不存在的知识包。"))?;
            let ids: HashSet<&str> = pack.cards.iter().filter_map(|c| c.id.as_deref()).collect();
            if excluded.iter().any(|id| !ids.contains(id.as_str())) {
                return Err(tr!("单题参与设置引用了不存在的题目。").into());
            }
        }
        Ok(())
    }

    pub fn card_answer_mode(&self, pack_id: &str, card_id: &str) -> AnswerMode {
        self.settings
            .card_answer_modes
            .get(pack_id)
            .and_then(|modes| modes.get(card_id))
            .copied()
            .unwrap_or_default()
    }

    pub fn uniform_answer_mode(&self, pack_id: &str) -> Option<AnswerMode> {
        let pack = self.packs.iter().find(|pack| pack.id == pack_id)?;
        let first = self.card_answer_mode(pack_id, pack.cards.first()?.id.as_deref()?);
        pack.cards
            .iter()
            .all(|card| {
                self.card_answer_mode(pack_id, card.id.as_deref().unwrap_or_default()) == first
            })
            .then_some(first)
    }

    pub fn set_card_answer_mode(
        &mut self,
        pack_id: &str,
        card_id: &str,
        mode: AnswerMode,
    ) -> AppResult<()> {
        let card = self
            .packs
            .iter()
            .find(|pack| pack.id == pack_id)
            .and_then(|pack| {
                pack.cards
                    .iter()
                    .find(|card| card.id.as_deref() == Some(card_id))
            })
            .ok_or(tr!("这道题已不存在，请重新选择。"))?;
        if !card.supports(mode) {
            return Err(match mode {
                AnswerMode::MultipleChoice => tr!("请为这道题补齐两个错误选项，再保存选择题设置。"),
                AnswerMode::TrueFalse => tr!("请为这道题补齐判断陈述，再保存判断题设置。"),
                AnswerMode::Input => unreachable!(),
            }
            .into());
        }
        if mode == AnswerMode::Input {
            if let Some(modes) = self.settings.card_answer_modes.get_mut(pack_id) {
                modes.remove(card_id);
                if modes.is_empty() {
                    self.settings.card_answer_modes.remove(pack_id);
                }
            }
        } else {
            self.settings
                .card_answer_modes
                .entry(pack_id.into())
                .or_default()
                .insert(card_id.into(), mode);
        }
        Ok(())
    }

    pub fn set_all_answer_modes(&mut self, id: &str, mode: AnswerMode) -> AppResult<()> {
        let pack = self
            .packs
            .iter()
            .find(|pack| pack.id == id)
            .ok_or(tr!("这个知识包已不存在，请重新选择。"))?;
        let missing = pack
            .cards
            .iter()
            .filter(|card| !card.supports(mode))
            .count();
        if missing > 0 {
            return Err(tr!(
                "有 {missing} 道题缺少{p0}内容，请点击题目补齐。",
                p0 = mode.label(),
                missing = missing
            ));
        }
        if mode == AnswerMode::Input {
            self.settings.card_answer_modes.remove(id);
        } else {
            let modes = pack
                .cards
                .iter()
                .map(|card| {
                    card.id
                        .clone()
                        .map(|id| (id, mode))
                        .ok_or_else(|| tr!("题目标识缺失，请重新载入题库。").to_owned())
                })
                .collect::<AppResult<_>>()?;
            self.settings.card_answer_modes.insert(id.into(), modes);
        }
        Ok(())
    }

    pub fn rename_pack(&mut self, id: &str, name: &str) -> AppResult<()> {
        let pack = self
            .packs
            .iter_mut()
            .find(|pack| pack.id == id)
            .ok_or(tr!("这个知识包已不存在，请重新选择。"))?;
        let name = name.trim();
        if name.is_empty() || name.chars().count() > 80 {
            return Err(tr!("知识包名称须为 1 至 80 个字符。").into());
        }
        pack.name = name.to_owned();
        Ok(())
    }

    pub fn set_pack_description(&mut self, id: &str, description: &str) -> AppResult<()> {
        let pack = self
            .packs
            .iter_mut()
            .find(|pack| pack.id == id)
            .ok_or(tr!("这个知识包已不存在，请重新选择。"))?;
        let description = description.trim();
        if description.chars().count() > 2000 {
            return Err(tr!("知识包介绍最多 2000 个字符。").into());
        }
        pack.description = description.to_owned();
        Ok(())
    }

    pub fn set_pack_tag(&mut self, id: &str, tag: &str) -> AppResult<()> {
        let pack = self
            .packs
            .iter_mut()
            .find(|pack| pack.id == id)
            .ok_or(tr!("这个知识包已不存在，请重新选择。"))?;
        validate_pack_tag(tag)?;
        pack.tag = tag.trim().to_owned();
        Ok(())
    }

    pub fn is_enabled(&self, id: &str) -> bool {
        self.settings
            .enabled_pack_ids
            .iter()
            .any(|enabled| enabled == id)
    }

    pub fn enabled_card_count(&self) -> usize {
        self.packs
            .iter()
            .map(|pack| self.pack_enabled_card_count(pack))
            .sum()
    }

    pub fn is_card_enabled(&self, pack_id: &str, card_id: &str) -> bool {
        self.is_enabled(pack_id)
            && !self
                .settings
                .excluded_card_ids
                .get(pack_id)
                .is_some_and(|ids| ids.contains(card_id))
    }

    pub fn pack_enabled_card_count(&self, pack: &KnowledgePack) -> usize {
        if !self.is_enabled(&pack.id) {
            return 0;
        }
        let excluded = self.settings.excluded_card_ids.get(&pack.id);
        pack.cards
            .iter()
            .filter(|card| {
                !excluded.is_some_and(|ids| card.id.as_ref().is_some_and(|id| ids.contains(id)))
            })
            .count()
    }

    pub fn set_card_enabled(
        &mut self,
        pack_id: &str,
        card_id: &str,
        enabled: bool,
    ) -> AppResult<()> {
        let pack = self
            .packs
            .iter()
            .find(|pack| pack.id == pack_id)
            .ok_or(tr!("这个知识包已不存在，请重新选择。"))?;
        if !pack
            .cards
            .iter()
            .any(|card| card.id.as_deref() == Some(card_id))
        {
            return Err(tr!("这道题已不存在，请重新选择。").into());
        }
        if !self.is_enabled(pack_id) {
            if !enabled {
                return Ok(());
            }
            let excluded = pack
                .cards
                .iter()
                .filter_map(|c| c.id.clone())
                .filter(|id| id != card_id)
                .collect();
            self.settings
                .excluded_card_ids
                .insert(pack_id.into(), excluded);
            self.settings.enabled_pack_ids.push(pack_id.into());
        } else {
            let ids = self
                .settings
                .excluded_card_ids
                .entry(pack_id.into())
                .or_default();
            if enabled {
                ids.remove(card_id);
            } else {
                ids.insert(card_id.into());
            }
        }
        if self
            .settings
            .excluded_card_ids
            .get(pack_id)
            .is_some_and(|ids| ids.len() == pack.cards.len())
        {
            self.settings.enabled_pack_ids.retain(|id| id != pack_id);
            self.settings.excluded_card_ids.remove(pack_id);
        } else if self
            .settings
            .excluded_card_ids
            .get(pack_id)
            .is_some_and(BTreeSet::is_empty)
        {
            self.settings.excluded_card_ids.remove(pack_id);
        }
        Ok(())
    }

    pub fn set_pack_enabled(&mut self, id: &str, enabled: bool) -> AppResult<()> {
        let pack = self
            .packs
            .iter()
            .find(|pack| pack.id == id)
            .ok_or(tr!("这个知识包已不存在，请重新选择。"))?;
        // If replacement removed every formerly participating question, an
        // explicit activation starts with all questions rather than an empty pool.
        if enabled
            && self.settings.excluded_card_ids.get(id).is_some_and(|ids| {
                pack.cards
                    .iter()
                    .all(|card| card.id.as_ref().is_some_and(|id| ids.contains(id)))
            })
        {
            self.settings.excluded_card_ids.remove(id);
        }
        self.settings.enabled_pack_ids.retain(|item| item != id);
        if enabled {
            self.settings.enabled_pack_ids.push(id.into());
        }
        Ok(())
    }

    pub fn set_all_cards_enabled(&mut self, id: &str, enabled: bool) -> AppResult<()> {
        self.set_pack_enabled(id, enabled)?;
        self.settings.excluded_card_ids.remove(id);
        Ok(())
    }

    pub fn delete_pack(&mut self, id: &str) -> AppResult<()> {
        let index = self
            .packs
            .iter()
            .position(|pack| pack.id == id)
            .ok_or(tr!("这个知识包已不存在，请重新选择。"))?;
        self.packs.remove(index);
        self.settings
            .enabled_pack_ids
            .retain(|pack_id| pack_id != id);
        self.settings.answer_modes.remove(id);
        self.settings.card_answer_modes.remove(id);
        self.settings.pack_practice_configs.remove(id);
        self.settings.excluded_card_ids.remove(id);
        Ok(())
    }

    pub fn import(&mut self, mut pack: KnowledgePack, replace: bool) -> AppResult<()> {
        pack.validate()?;
        let mut candidate = self.clone();
        if let Some(index) = candidate.packs.iter().position(|item| item.id == pack.id) {
            if !replace {
                return Err(tr!("此 id 的知识包已存在，请确认后替换。").into());
            }
            pack.assign_card_ids(Some(&candidate.packs[index]));
            let previous = &candidate.packs[index];
            let all_selected = candidate.pack_enabled_card_count(previous) == previous.cards.len();
            let known: HashSet<&str> = previous
                .cards
                .iter()
                .filter_map(|card| card.id.as_deref())
                .collect();
            let previously_excluded = candidate.settings.excluded_card_ids.get(&pack.id);
            let excluded: BTreeSet<String> = pack
                .cards
                .iter()
                .filter_map(|card| card.id.as_ref())
                .filter(|id| {
                    previously_excluded.is_some_and(|ids| ids.contains(*id))
                        || (!all_selected && !known.contains(id.as_str()))
                })
                .cloned()
                .collect();
            if excluded.is_empty() {
                candidate.settings.excluded_card_ids.remove(&pack.id);
            } else {
                candidate
                    .settings
                    .excluded_card_ids
                    .insert(pack.id.clone(), excluded);
            }
            if let Some(modes) = candidate.settings.card_answer_modes.get_mut(&pack.id) {
                modes.retain(|id, mode| {
                    pack.cards
                        .iter()
                        .any(|card| card.id.as_ref() == Some(id) && card.supports(*mode))
                });
                if modes.is_empty() {
                    candidate.settings.card_answer_modes.remove(&pack.id);
                }
            }
            candidate.packs[index] = pack;
        } else {
            pack.assign_card_ids(None);
            candidate.settings.enabled_pack_ids.push(pack.id.clone());
            candidate.packs.push(pack);
        }
        candidate.validate()?;
        *self = candidate;
        Ok(())
    }

    pub fn save_card(&mut self, pack_id: &str, card_id: Option<&str>, card: Card) -> AppResult<()> {
        let mode = card_id
            .map(|id| self.card_answer_mode(pack_id, id))
            .unwrap_or_default();
        self.save_card_with_mode(pack_id, card_id, card, mode)
    }

    pub fn save_card_with_mode(
        &mut self,
        pack_id: &str,
        card_id: Option<&str>,
        mut card: Card,
        mode: AnswerMode,
    ) -> AppResult<()> {
        let index = self
            .packs
            .iter()
            .position(|pack| pack.id == pack_id)
            .ok_or(tr!("这个知识包已不存在，请重新选择。"))?;
        let mut pack = self.packs[index].clone();
        let was_all_selected = self.pack_enabled_card_count(&pack) == pack.cards.len();
        let adding = card_id.is_none();
        if let Some(id) = card_id {
            let target = pack
                .cards
                .iter_mut()
                .find(|card| card.id.as_deref() == Some(id))
                .ok_or(tr!("这道题已不存在，请重新选择。"))?;
            card.id = target.id.clone();
            *target = card;
        } else {
            card.id = None;
            pack.cards.push(card);
        }
        pack.validate()?;
        pack.assign_card_ids(None);
        let added_id = adding.then(|| pack.cards.last().unwrap().id.clone().unwrap());
        let target_id = card_id.or(added_id.as_deref()).unwrap().to_owned();
        let mut candidate = self.clone();
        candidate.packs[index] = pack;
        candidate.set_card_answer_mode(pack_id, &target_id, mode)?;
        if let Some(id) = added_id
            && !was_all_selected
        {
            candidate
                .settings
                .excluded_card_ids
                .entry(pack_id.into())
                .or_default()
                .insert(id);
        }
        candidate.validate()?;
        *self = candidate;
        Ok(())
    }

    pub fn delete_card(&mut self, pack_id: &str, card_id: &str) -> AppResult<()> {
        let pack_index = self
            .packs
            .iter()
            .position(|pack| pack.id == pack_id)
            .ok_or(tr!("这个知识包已不存在，请重新选择。"))?;
        let card_index = self.packs[pack_index]
            .cards
            .iter()
            .position(|card| card.id.as_deref() == Some(card_id))
            .ok_or(tr!("这道题已不存在，请重新选择。"))?;
        let mut candidate = self.clone();
        candidate.packs[pack_index].cards.remove(card_index);
        if let Some(modes) = candidate.settings.card_answer_modes.get_mut(pack_id) {
            modes.remove(card_id);
            if modes.is_empty() {
                candidate.settings.card_answer_modes.remove(pack_id);
            }
        }
        if let Some(excluded) = candidate.settings.excluded_card_ids.get_mut(pack_id) {
            excluded.remove(card_id);
            if excluded.is_empty() {
                candidate.settings.excluded_card_ids.remove(pack_id);
            }
        }
        if candidate.packs[pack_index].cards.is_empty() {
            candidate.settings.answer_modes.remove(pack_id);
            candidate.settings.pack_practice_configs.remove(pack_id);
        }
        candidate.validate()?;
        *self = candidate;
        Ok(())
    }

    /// Only locally identified, participating cards are eligible for selection.
    /// Selection policy belongs to practice, not to persisted knowledge content.
    pub fn participating_cards(&self) -> impl Iterator<Item = (&KnowledgePack, &Card)> {
        self.packs
            .iter()
            .filter(move |pack| self.is_enabled(&pack.id))
            .flat_map(move |pack| {
                pack.cards
                    .iter()
                    .filter(move |card| {
                        card.id
                            .as_deref()
                            .is_some_and(|id| self.is_card_enabled(&pack.id, id))
                    })
                    .map(move |card| (pack, card))
            })
    }
}

fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 80
        && id.bytes().next().is_some_and(|c| c.is_ascii_lowercase())
        && id
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'_')
}

fn card_content_key(card: &Card) -> String {
    let mut content = card.clone();
    content.id = None;
    serde_json::to_string(&content).expect("card strings serialize")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_prompts_require_a_valid_image_and_answers_stay_required() {
        let mut pack = crate::test_fixtures::regression_library().packs.remove(0);
        let card = &mut pack.cards[0];
        card.prompt.clear();
        card.prompt_latex = None;
        assert!(pack.validate().is_err());
        pack.cards[0].prompt_image = Some("not_an_image".into());
        assert!(pack.validate().is_err());
        pack.cards[0].prompt_image = Some("a".repeat(64));
        assert!(pack.validate().is_ok());
        let bytes = serde_json::to_vec(&pack).unwrap();
        assert_eq!(KnowledgePack::from_json(&bytes).unwrap(), pack);
        pack.cards[0].answer.clear();
        assert!(pack.validate().is_err());
        pack.cards[0].answer = "sin(x)".into();
        pack.cards[0].prompt = "a".repeat(1001);
        assert!(pack.validate().is_err());
    }

    #[test]
    fn starter_packs_are_importable_and_support_all_answer_modes_across_domains() {
        let library = Library::default();
        library.validate().unwrap();
        assert_eq!(library.packs.len(), 5);
        assert_eq!(library.enabled_card_count(), 20);
        let mut rng = rand::rng();
        for pack in &library.packs {
            assert!(library.is_enabled(&pack.id));
            assert_eq!(pack.cards.len(), 4);
            let bytes = serde_json::to_vec_pretty(pack).unwrap();
            assert_eq!(KnowledgePack::from_json(&bytes).unwrap(), *pack);
            for card in &pack.cards {
                assert!(card.id.is_some());
                assert!(card.accepts(&card.answer));
                let math_hint = card.prompt_latex.is_some() || card.answer_latex.is_some();
                assert!(crate::math_answer::incomplete_error(&card.answer, math_hint).is_none());
                if pack.id == "programming_recall" {
                    assert!(crate::math_answer::recognize_inline(&card.answer, false).is_none());
                }
                for tex in [&card.prompt_latex, &card.answer_latex]
                    .into_iter()
                    .flatten()
                {
                    let image = crate::formula::render(tex, 16, 1.0, (235, 235, 235)).unwrap();
                    assert!(image.width > 0.0 && image.height > 0.0);
                }
                for alias in &card.aliases {
                    assert!(card.accepts(alias), "{}: {alias}", pack.id);
                }
                for distractor in &card.distractors {
                    assert!(!card.accepts(&distractor.text));
                }
                for mode in AnswerMode::ALL {
                    let prepared =
                        crate::practice::PreparedQuestion::new(card, mode, &mut rng).unwrap();
                    assert!(!prepared.prompt.text.is_empty());
                }
                crate::practice::PreparedQuestion::knowledge(card);
            }
        }
        assert!(
            library.packs[0]
                .cards
                .iter()
                .any(|c| c.prompt_latex.is_some())
        );
        assert!(
            library.packs[1]
                .cards
                .iter()
                .all(|c| c.prompt_latex.is_none())
        );
        assert!(
            library.packs[3]
                .cards
                .iter()
                .any(|c| c.prompt_latex.is_some())
        );
    }

    #[test]
    fn deleting_questions_preserves_content_and_cleans_only_their_preferences() {
        for enabled in [true, false] {
            let mut library = crate::test_fixtures::regression_library();
            let pack_id = library.packs[0].id.clone();
            let removed = library.packs[0].cards[0].id.clone().unwrap();
            let retained = library.packs[0].cards[1].id.clone().unwrap();
            library
                .set_all_answer_modes(&pack_id, AnswerMode::TrueFalse)
                .unwrap();
            library.set_card_enabled(&pack_id, &removed, false).unwrap();
            library
                .set_card_enabled(&pack_id, &retained, false)
                .unwrap();
            library.set_pack_enabled(&pack_id, enabled).unwrap();
            let before = library.clone();
            library.delete_card(&pack_id, &removed).unwrap();
            assert_eq!(library.packs[0].cards, before.packs[0].cards[1..]);
            assert_eq!(library.packs[0].name, before.packs[0].name);
            assert_eq!(
                library.settings.enabled_pack_ids,
                before.settings.enabled_pack_ids
            );
            assert_eq!(library.settings.card_answer_modes[&pack_id].len(), 19);
            assert!(!library.settings.card_answer_modes[&pack_id].contains_key(&removed));
            assert_eq!(
                library.settings.excluded_card_ids[&pack_id],
                BTreeSet::from([retained])
            );
            let after = library.clone();
            assert!(library.delete_card(&pack_id, &removed).is_err());
            assert!(library.delete_card("missing_pack", &removed).is_err());
            assert_eq!(library, after);
        }
    }

    #[test]
    fn deleting_all_questions_keeps_an_empty_pack_and_allows_adding_again() {
        let mut library = crate::test_fixtures::regression_library();
        let original = library.packs[0].clone();
        library
            .set_all_answer_modes(&original.id, AnswerMode::MultipleChoice)
            .unwrap();
        for card in &original.cards {
            library
                .delete_card(&original.id, card.id.as_deref().unwrap())
                .unwrap();
        }
        assert_eq!(library.packs.len(), 1);
        assert_eq!(library.packs[0].id, original.id);
        assert_eq!(library.packs[0].name, original.name);
        assert_eq!(library.enabled_card_count(), 0);
        assert!(library.settings.card_answer_modes.is_empty());
        assert!(library.settings.excluded_card_ids.is_empty());
        assert!(library.packs[0].available_modes().is_empty());
        library.validate().unwrap();
        library
            .save_card(&original.id, None, original.cards[0].clone())
            .unwrap();
        assert_eq!(library.packs[0].cards.len(), 1);
        assert_eq!(library.enabled_card_count(), 1);
        assert_eq!(
            library.uniform_answer_mode(&original.id),
            Some(AnswerMode::Input)
        );
    }

    #[test]
    fn deleting_a_pack_removes_its_preferences_and_preserves_the_others() {
        for participating in [true, false] {
            let mut library = crate::test_fixtures::regression_library();
            let removed = library.packs[0].id.clone();
            let mut retained = library.packs[0].clone();
            retained.id = "retained_pack".into();
            library.import(retained.clone(), false).unwrap();
            library.settings.interval_minutes = 47;
            for id in [&removed, &retained.id] {
                library
                    .set_all_answer_modes(id, AnswerMode::TrueFalse)
                    .unwrap();
                library
                    .set_card_enabled(id, retained.cards[0].id.as_deref().unwrap(), false)
                    .unwrap();
                library
                    .settings
                    .answer_modes
                    .insert(id.clone(), AnswerMode::MultipleChoice);
                library
                    .settings
                    .pack_practice_configs
                    .insert(id.clone(), PracticeConfig::TrueFalse);
            }
            library.set_pack_enabled(&removed, participating).unwrap();
            let retained_modes = library.settings.card_answer_modes[&retained.id].clone();
            let retained_range = library.settings.excluded_card_ids[&retained.id].clone();

            library.delete_pack(&removed).unwrap();
            library.validate().unwrap();
            assert_eq!(library.packs, [retained.clone()]);
            assert_eq!(library.settings.enabled_pack_ids, [retained.id.clone()]);
            assert!(!library.settings.answer_modes.contains_key(&removed));
            assert!(!library.settings.card_answer_modes.contains_key(&removed));
            assert!(
                !library
                    .settings
                    .pack_practice_configs
                    .contains_key(&removed)
            );
            assert!(!library.settings.excluded_card_ids.contains_key(&removed));
            assert_eq!(
                library.settings.card_answer_modes[&retained.id],
                retained_modes
            );
            assert_eq!(
                library.settings.excluded_card_ids[&retained.id],
                retained_range
            );
            assert_eq!(
                library.settings.answer_modes[&retained.id],
                AnswerMode::MultipleChoice
            );
            assert_eq!(
                library.settings.pack_practice_configs[&retained.id],
                PracticeConfig::TrueFalse
            );
            assert_eq!(library.settings.interval_minutes, 47);
            assert_eq!(choose(&library, None).unwrap().key.pack_id, retained.id);
        }
    }

    #[test]
    fn deleting_an_unknown_pack_keeps_the_library_intact() {
        let mut library = crate::test_fixtures::regression_library();
        let before = library.clone();
        assert!(library.delete_pack("missing_pack").is_err());
        assert_eq!(library, before);
    }

    fn choose(
        library: &Library,
        previous: Option<&crate::practice::QuestionKey>,
    ) -> Option<crate::practice::Question> {
        crate::practice::select_question(library, previous, &mut rand::rng())
    }

    #[test]
    fn an_input_question_can_be_added_beside_a_judgment_question() {
        let mut library = crate::test_fixtures::regression_library();
        library.packs[0].cards.truncate(1);
        let pack_id = library.packs[0].id.clone();
        let card_id = library.packs[0].cards[0].id.clone().unwrap();
        library
            .set_card_answer_mode(&pack_id, &card_id, AnswerMode::TrueFalse)
            .unwrap();
        let input = crate::card_editor::CardDraft {
            prompt: "尝试的英文？".into(),
            answer: "try".into(),
            ..Default::default()
        }
        .into_card();
        let result = library.save_card(&pack_id, None, input);
        assert!(
            result.is_ok(),
            "Adding an input question must not require changing another question's mode: {result:?}"
        );
        let original = library.packs[0].cards[0].id.as_deref().unwrap();
        let added = library.packs[0].cards[1].id.as_deref().unwrap();
        assert_eq!(
            library.card_answer_mode(&pack_id, original),
            AnswerMode::TrueFalse
        );
        assert_eq!(library.card_answer_mode(&pack_id, added), AnswerMode::Input);
        assert_eq!(library.uniform_answer_mode(&pack_id), None);
    }

    #[test]
    fn bulk_modes_write_each_question_without_touching_participation() {
        let mut library = crate::test_fixtures::regression_library();
        let pack_id = library.packs[0].id.clone();
        let ids: Vec<_> = library.packs[0]
            .cards
            .iter()
            .take(3)
            .map(|c| c.id.clone().unwrap())
            .collect();
        library.set_card_enabled(&pack_id, &ids[1], false).unwrap();
        let participation = library.settings.excluded_card_ids.clone();
        library
            .set_card_answer_mode(&pack_id, &ids[1], AnswerMode::MultipleChoice)
            .unwrap();
        library
            .set_card_answer_mode(&pack_id, &ids[2], AnswerMode::TrueFalse)
            .unwrap();
        assert_eq!(
            library.card_answer_mode(&pack_id, &ids[0]),
            AnswerMode::Input
        );
        assert_eq!(
            library.card_answer_mode(&pack_id, &ids[1]),
            AnswerMode::MultipleChoice
        );
        assert_eq!(
            library.card_answer_mode(&pack_id, &ids[2]),
            AnswerMode::TrueFalse
        );
        assert_eq!(library.uniform_answer_mode(&pack_id), None);
        for mode in AnswerMode::ALL {
            library.set_all_answer_modes(&pack_id, mode).unwrap();
            for id in &ids {
                assert_eq!(library.card_answer_mode(&pack_id, id), mode);
            }
            assert_eq!(library.uniform_answer_mode(&pack_id), Some(mode));
            assert_eq!(library.settings.excluded_card_ids, participation);
            let restored: Library =
                serde_json::from_slice(&serde_json::to_vec(&library).unwrap()).unwrap();
            restored.validate().unwrap();
            assert_eq!(restored, library);
        }
        library
            .set_card_answer_mode(&pack_id, &ids[0], AnswerMode::Input)
            .unwrap();
        assert_eq!(
            library.card_answer_mode(&pack_id, &ids[1]),
            AnswerMode::TrueFalse
        );
        assert!(library.settings.pack_practice_configs.is_empty());
        assert_eq!(library.uniform_answer_mode(&pack_id), None);
        assert_eq!(library.settings.excluded_card_ids, participation);
    }

    #[test]
    fn missing_mode_content_rejects_the_whole_change_and_can_be_saved_with_content() {
        let mut library = crate::test_fixtures::regression_library();
        let pack_id = library.packs[0].id.clone();
        let id = library.packs[0].cards[0].id.clone().unwrap();
        let mut card = library.packs[0].cards[0].clone();
        let distractors = std::mem::take(&mut card.distractors);
        library
            .save_card(&pack_id, Some(&id), card.clone())
            .unwrap();
        let original = library.clone();
        assert!(
            library
                .set_card_answer_mode(&pack_id, &id, AnswerMode::MultipleChoice)
                .is_err()
        );
        assert_eq!(library, original);
        assert!(
            library
                .set_all_answer_modes(&pack_id, AnswerMode::MultipleChoice)
                .unwrap_err()
                .contains("1 道题")
        );
        assert_eq!(
            library, original,
            "A practice configuration cannot partially apply"
        );
        card.distractors = distractors;
        library
            .save_card_with_mode(&pack_id, Some(&id), card, AnswerMode::MultipleChoice)
            .unwrap();
        assert_eq!(
            library.card_answer_mode(&pack_id, &id),
            AnswerMode::MultipleChoice
        );
        assert_eq!(
            library.card_answer_mode(&pack_id, library.packs[0].cards[1].id.as_deref().unwrap()),
            AnswerMode::Input
        );
    }

    #[test]
    fn participation_supports_all_none_and_one_without_changing_content() {
        let mut library = crate::test_fixtures::regression_library();
        let pack = library.packs[0].clone();
        let id = pack.cards[3].id.as_deref().unwrap();
        library.set_pack_enabled(&pack.id, false).unwrap();
        assert!(choose(&library, None).is_none());
        library.set_card_enabled(&pack.id, id, true).unwrap();
        assert_eq!(library.enabled_card_count(), 1);
        let question = choose(&library, None).unwrap();
        assert_eq!(question.key.card_id, id);
        assert_eq!(
            choose(&library, Some(&question.key)).unwrap().key,
            question.key
        );
        library.set_card_enabled(&pack.id, id, false).unwrap();
        assert_eq!(library.enabled_card_count(), 0);
        library.set_pack_enabled(&pack.id, true).unwrap();
        assert_eq!(library.enabled_card_count(), pack.cards.len());
        assert_eq!(library.packs[0], pack);
    }

    #[test]
    fn pack_switch_restores_saved_range_and_modes_without_affecting_other_packs() {
        let directory = tempfile::tempdir().unwrap();
        let store = crate::storage::Store::open(directory.path().to_owned()).unwrap();
        let mut library = crate::test_fixtures::regression_library();
        store.save(&library).unwrap();
        let pack_id = library.packs[0].id.clone();
        let card_id = library.packs[0].cards[0].id.clone().unwrap();
        let mut other = library.packs[0].clone();
        other.id = "second_pack".into();
        other.cards.truncate(3);
        library.import(other, false).unwrap();
        library.set_card_enabled(&pack_id, &card_id, false).unwrap();
        library
            .set_card_answer_mode(&pack_id, &card_id, AnswerMode::MultipleChoice)
            .unwrap();
        library
            .set_card_answer_mode("second_pack", &card_id, AnswerMode::TrueFalse)
            .unwrap();
        let content = library.packs.clone();
        let excluded = library.settings.excluded_card_ids.clone();
        let modes = library.settings.card_answer_modes.clone();
        library.set_pack_enabled(&pack_id, false).unwrap();
        assert_eq!(library.enabled_card_count(), 3);
        assert!(
            library
                .participating_cards()
                .all(|(pack, _)| pack.id == "second_pack")
        );
        store.save(&library).unwrap();
        let mut restored = store.load().unwrap();
        assert!(!restored.is_enabled(&pack_id));
        restored.set_pack_enabled(&pack_id, true).unwrap();
        assert_eq!(
            restored.enabled_card_count(),
            content[0].cards.len() - 1 + 3
        );
        assert!(!restored.is_card_enabled(&pack_id, &card_id));
        assert_eq!(restored.settings.excluded_card_ids, excluded);
        assert_eq!(restored.settings.card_answer_modes, modes);
        assert_eq!(restored.packs, content);
        restored.set_all_cards_enabled(&pack_id, true).unwrap();
        assert!(restored.is_card_enabled(&pack_id, &card_id));
        assert_eq!(restored.settings.card_answer_modes, modes);
        assert_eq!(restored.pack_enabled_card_count(&restored.packs[1]), 3);
        restored.validate().unwrap();
    }

    #[test]
    fn disabled_pack_keeps_its_range_when_questions_are_reimported_or_added() {
        for partial in [false, true] {
            for legacy in [false, true] {
                let mut library = crate::test_fixtures::regression_library();
                let original = library.packs[0].clone();
                let pack_id = original.id.clone();
                let selected = original.cards[0].id.clone().unwrap();
                if partial {
                    library.set_all_cards_enabled(&pack_id, false).unwrap();
                    library.set_card_enabled(&pack_id, &selected, true).unwrap();
                }
                library
                    .set_card_answer_mode(&pack_id, &selected, AnswerMode::MultipleChoice)
                    .unwrap();
                library.set_pack_enabled(&pack_id, false).unwrap();
                let mut replacement = original.clone();
                let mut added = replacement.cards[0].clone();
                added.id = Some("new_imported_question".into());
                added.prompt = "A new question?".into();
                replacement.cards.push(added);
                replacement.cards.reverse();
                if legacy {
                    replacement.cards.iter_mut().for_each(|card| card.id = None);
                }
                library.import(replacement, true).unwrap();
                library
                    .save_card(&pack_id, None, original.cards[0].clone())
                    .unwrap();
                assert_eq!(library.enabled_card_count(), 0);
                library.set_pack_enabled(&pack_id, true).unwrap();
                assert_eq!(
                    library.enabled_card_count(),
                    if partial { 1 } else { original.cards.len() },
                    "partial={partial}, legacy={legacy}"
                );
                assert!(library.is_card_enabled(&pack_id, &selected));
                assert_eq!(
                    library.card_answer_mode(&pack_id, &selected),
                    AnswerMode::MultipleChoice
                );
                library.validate().unwrap();
            }
        }
    }

    #[test]
    fn explicit_pack_activation_recovers_after_its_selected_question_is_removed() {
        let mut library = crate::test_fixtures::regression_library();
        let pack_id = library.packs[0].id.clone();
        let selected = library.packs[0].cards[0].id.clone().unwrap();
        library.set_all_cards_enabled(&pack_id, false).unwrap();
        library.set_card_enabled(&pack_id, &selected, true).unwrap();
        library.set_pack_enabled(&pack_id, false).unwrap();
        let mut replacement = library.packs[0].clone();
        replacement.cards.remove(0);
        library.import(replacement, true).unwrap();
        assert_eq!(library.enabled_card_count(), 0);
        library.set_pack_enabled(&pack_id, true).unwrap();
        assert_eq!(library.enabled_card_count(), library.packs[0].cards.len());
        library.validate().unwrap();
    }

    #[test]
    fn replacement_tracks_ids_across_reordering_and_legacy_content() {
        let mut library = crate::test_fixtures::regression_library();
        let pack_id = library.packs[0].id.clone();
        let excluded = library.packs[0].cards[2].id.clone().unwrap();
        library
            .set_card_enabled(&pack_id, &excluded, false)
            .unwrap();
        library
            .set_card_answer_mode(&pack_id, &excluded, AnswerMode::MultipleChoice)
            .unwrap();
        let mut replacement = library.packs[0].clone();
        replacement.cards.reverse();
        library.import(replacement.clone(), true).unwrap();
        assert!(!library.is_card_enabled(&pack_id, &excluded));
        assert_eq!(
            library.card_answer_mode(&pack_id, &excluded),
            AnswerMode::MultipleChoice
        );
        for card in &mut replacement.cards {
            card.id = None;
        }
        library.import(replacement, true).unwrap();
        assert!(!library.is_card_enabled(&pack_id, &excluded));
        assert_eq!(
            library.card_answer_mode(&pack_id, &excluded),
            AnswerMode::MultipleChoice
        );
        assert_eq!(library.enabled_card_count(), 19);
        let mut removed = library.packs[0].clone();
        removed
            .cards
            .retain(|card| card.id.as_deref() != Some(&excluded));
        library.import(removed, true).unwrap();
        library.validate().unwrap();
        assert_eq!(library.enabled_card_count(), 19);
    }

    #[test]
    fn editing_preserves_identity_participation_and_other_question_data() {
        let mut library = crate::test_fixtures::regression_library();
        let pack_id = library.packs[0].id.clone();
        let id = library.packs[0].cards[0].id.clone().unwrap();
        library.set_card_enabled(&pack_id, &id, false).unwrap();
        let other = library.packs[0].cards[1].clone();
        let mut card = library.packs[0].cards[0].clone();
        card.prompt = "新题面".into();
        library.save_card(&pack_id, Some(&id), card).unwrap();
        assert_eq!(library.packs[0].cards[0].id.as_deref(), Some(id.as_str()));
        assert!(!library.is_card_enabled(&pack_id, &id));
        assert_eq!(library.packs[0].cards[1], other);
        let original = library.clone();
        let mut invalid = library.packs[0].cards[0].clone();
        invalid.answer.clear();
        assert!(library.save_card(&pack_id, Some(&id), invalid).is_err());
        assert_eq!(library, original);
        library
            .set_card_answer_mode(&pack_id, &id, AnswerMode::TrueFalse)
            .unwrap();
        let original = library.clone();
        let mut plain = library.packs[0].cards[0].clone();
        plain.true_false = None;
        assert!(library.save_card(&pack_id, Some(&id), plain).is_err());
        assert_eq!(
            library, original,
            "Editing does not reset the question's existing answer mode"
        );
    }

    #[test]
    fn new_questions_do_not_expand_a_partial_practice_selection() {
        let mut library = crate::test_fixtures::regression_library();
        let pack_id = library.packs[0].id.clone();
        let id = library.packs[0].cards[0].id.clone().unwrap();
        library.set_pack_enabled(&pack_id, false).unwrap();
        library.set_card_enabled(&pack_id, &id, true).unwrap();
        let new = library.packs[0].cards[0].clone();
        library.save_card(&pack_id, None, new).unwrap();
        assert_eq!(library.packs[0].cards.len(), 21);
        assert_eq!(library.enabled_card_count(), 1);
        assert_eq!(choose(&library, None).unwrap().key.card_id, id);
    }

    #[test]
    fn replacement_does_not_enroll_new_questions_into_a_partial_selection() {
        for legacy in [false, true] {
            let mut library = crate::test_fixtures::regression_library();
            let pack_id = library.packs[0].id.clone();
            let selected = library.packs[0].cards[0].id.clone().unwrap();
            library.set_pack_enabled(&pack_id, false).unwrap();
            library.set_card_enabled(&pack_id, &selected, true).unwrap();
            let mut replacement = library.packs[0].clone();
            let mut added = replacement.cards[0].clone();
            added.id = Some("new_question".into());
            added.prompt = "A new question?".into();
            replacement.cards.push(added);
            replacement.cards.reverse();
            if legacy {
                for card in &mut replacement.cards {
                    card.id = None;
                }
            }
            library.import(replacement, true).unwrap();
            assert_eq!(library.enabled_card_count(), 1, "legacy={legacy}");
            assert_eq!(choose(&library, None).unwrap().key.card_id, selected);
            library.validate().unwrap();
        }

        let mut library = crate::test_fixtures::regression_library();
        let mut replacement = library.packs[0].clone();
        let mut added = replacement.cards[0].clone();
        added.id = Some("new_question".into());
        replacement.cards.push(added);
        library.import(replacement.clone(), true).unwrap();
        assert_eq!(library.enabled_card_count(), replacement.cards.len());
        library.set_pack_enabled(&replacement.id, false).unwrap();
        library.import(replacement, true).unwrap();
        assert_eq!(library.enabled_card_count(), 0);
    }

    #[test]
    fn duplicate_formula_options_cannot_be_imported() {
        for duplicate_correct_answer in [true, false] {
            let mut library = crate::test_fixtures::regression_library();
            let before = library.clone();
            let mut replacement = library.packs[0].clone();
            let card = &mut replacement.cards[0];
            card.answer = "1/2".into();
            card.answer_latex = Some(r"\frac{1}{2}".into());
            card.aliases.clear();
            card.distractors = if duplicate_correct_answer {
                vec![
                    AnswerContent {
                        text: "½".into(),
                        latex: Some(r"\frac{1}{2}".into()),
                    },
                    AnswerContent {
                        text: "2".into(),
                        latex: Some("2".into()),
                    },
                ]
            } else {
                vec![
                    AnswerContent {
                        text: "x²".into(),
                        latex: Some("x^2".into()),
                    },
                    AnswerContent {
                        text: "x^2".into(),
                        latex: Some("x^{2}".into()),
                    },
                ]
            };
            assert!(
                library.import(replacement, true).is_err(),
                "duplicate_correct_answer={duplicate_correct_answer}"
            );
            assert_eq!(library, before);
        }
    }

    #[test]
    fn accepts_declared_spellings_without_guessing() {
        let card = &crate::test_fixtures::regression_library().packs[0].cards[2];
        assert!(card.accepts("  √3/2\n"));
        assert!(card.accepts("sqrt(3)/2"));
        assert!(!card.accepts("0.866"));
        assert!(!card.accepts("SQRT(3)/2"));
        assert!(!card.accepts(""));
    }

    #[test]
    fn import_round_trip_preserves_unicode_and_aliases() {
        let pack = crate::test_fixtures::regression_library().packs.remove(0);
        let bytes = serde_json::to_vec_pretty(&pack).unwrap();
        assert_eq!(KnowledgePack::from_json(&bytes).unwrap(), pack);
        let mut bom = vec![0xef, 0xbb, 0xbf];
        bom.extend_from_slice(&bytes);
        assert_eq!(KnowledgePack::from_json(&bom).unwrap(), pack);
    }

    #[test]
    fn optional_math_display_round_trips_and_accepts_formula_notation() {
        let mut pack = crate::test_fixtures::regression_library().packs.remove(0);
        let card = &mut pack.cards[2];
        card.prompt_latex = Some(r"\sin\left(\frac{\pi}{3}\right)=?".into());
        card.answer_latex = Some(r"\frac{\sqrt{3}}{2}".into());
        assert!(card.accepts("sqrt(3)/2"));
        assert!(card.accepts(r"\frac{\sqrt{3}}{2}"));
        assert_eq!(
            KnowledgePack::from_json(&serde_json::to_vec(&pack).unwrap()).unwrap(),
            pack
        );
        let mut library = crate::test_fixtures::regression_library();
        let original = library.clone();
        pack.cards[2].prompt_latex = Some(r"\tryunknown{x}".into());
        assert!(library.import(pack, true).unwrap_err().contains("第 3 题"));
        assert_eq!(library, original);
    }

    #[test]
    fn plain_text_cards_need_no_math_fields() {
        let pack = KnowledgePack::from_json(br#"{"format":"try","schema_version":1,"id":"words","name":"Words","cards":[{"prompt":"A word?","answer":"try"}]}"#).unwrap();
        assert_eq!(pack.cards[0].prompt_latex, None);
        let exported = serde_json::to_string(&pack).unwrap();
        assert!(!exported.contains("latex"));
        assert_eq!(pack.available_modes(), vec![AnswerMode::Input]);
        assert!(pack.cards[0].distractors.is_empty());
        assert_eq!(pack.cards[0].true_false, None);
    }

    #[test]
    fn modes_persist_and_replacement_falls_back_if_content_is_missing() {
        let mut library = crate::test_fixtures::regression_library();
        let ids: Vec<_> = library.packs[0]
            .cards
            .iter()
            .map(|card| card.id.clone().unwrap())
            .collect();
        for id in &ids {
            library
                .set_card_answer_mode("test_pack", id, AnswerMode::MultipleChoice)
                .unwrap();
        }
        let restored: Library =
            serde_json::from_slice(&serde_json::to_vec(&library).unwrap()).unwrap();
        restored.validate().unwrap();
        assert_eq!(
            restored.uniform_answer_mode("test_pack"),
            Some(AnswerMode::MultipleChoice)
        );
        let mut replacement = library.packs[0].clone();
        replacement.cards[0].distractors.clear();
        assert!(!replacement.supports(AnswerMode::MultipleChoice));
        library.import(replacement, true).unwrap();
        assert_eq!(
            library.card_answer_mode(
                "test_pack",
                library.packs[0].cards[0].id.as_deref().unwrap()
            ),
            AnswerMode::Input
        );
        assert_eq!(
            library.card_answer_mode(
                "test_pack",
                library.packs[0].cards[1].id.as_deref().unwrap()
            ),
            AnswerMode::MultipleChoice
        );
        assert_eq!(library.uniform_answer_mode("test_pack"), None);
        assert!(
            library
                .set_all_answer_modes("test_pack", AnswerMode::MultipleChoice)
                .is_err()
        );
        library
            .set_all_answer_modes("test_pack", AnswerMode::TrueFalse)
            .unwrap();
        let mut replacement = library.packs[0].clone();
        replacement.cards[0].true_false = None;
        library.import(replacement, true).unwrap();
        assert_eq!(
            library.card_answer_mode("test_pack", &ids[0]),
            AnswerMode::Input
        );
        assert_eq!(
            library.card_answer_mode("test_pack", &ids[1]),
            AnswerMode::TrueFalse
        );
    }

    #[test]
    fn invalid_choice_or_statement_import_preserves_library() {
        let mut library = crate::test_fixtures::regression_library();
        let original = library.clone();
        let mut pack = library.packs[0].clone();
        pack.cards[0].distractors[0].text = " 0.5 ".into();
        assert!(
            library.import(pack, true).is_err(),
            "An accepted alias cannot be a distractor"
        );
        let mut pack = library.packs[0].clone();
        pack.cards[0].distractors[1] = pack.cards[0].distractors[0].clone();
        assert!(library.import(pack, true).is_err());
        let mut pack = library.packs[0].clone();
        pack.cards[0].true_false.as_mut().unwrap().correction_latex =
            Some(r"\tryunknown{x}".into());
        assert!(library.import(pack, true).is_err());
        assert_eq!(library, original);
    }

    #[test]
    fn old_settings_load_with_floating_defaults() {
        let mut settings: Settings = serde_json::from_str(r#"{"reminders_enabled":true,"interval_minutes":30,"feedback_seconds":8,"enabled_pack_ids":["test_pack"]}"#).unwrap();
        assert_eq!(settings.reminders_enabled.take(), Some(true));
        assert_eq!(
            settings,
            Settings {
                enabled_pack_ids: vec!["test_pack".into()],
                ..Settings::default()
            }
        );
    }

    #[test]
    fn appearance_defaults_to_system_and_preserves_saved_manual_choices() {
        let settings: Settings = serde_json::from_str("{}").unwrap();
        assert_eq!(settings.appearance, Appearance::System);
        for (value, appearance, index) in [
            ("light", Appearance::Light, 0),
            ("dark", Appearance::Dark, 1),
            ("system", Appearance::System, 2),
        ] {
            let settings = Settings {
                appearance,
                ..Settings::default()
            };
            let encoded = serde_json::to_string(&settings).unwrap();
            assert!(encoded.contains(&format!("\"appearance\":\"{value}\"")));
            assert_eq!(
                serde_json::from_str::<Settings>(&encoded)
                    .unwrap()
                    .appearance,
                appearance
            );
            assert_eq!(Appearance::from_index(index), Some(appearance));
        }
        assert_eq!(Appearance::from_index(99), None);
    }

    #[test]
    fn invalid_imported_tags_preserve_the_library_and_accept_unicode_limits() {
        let mut library = Library::default();
        let before = library.clone();
        for tag in [
            "字".repeat(25),
            "数学\n化学".into(),
            "bad\0tag".into(),
            "数学\u{2028}化学".into(),
            "数学\u{2029}化学".into(),
        ] {
            let mut pack = before.packs[0].clone();
            pack.tag = tag;
            assert!(KnowledgePack::from_json(&serde_json::to_vec(&pack).unwrap()).is_err());
            assert!(library.import(pack, true).is_err());
            assert_eq!(library, before);
        }
        let mut pack = before.packs[0].clone();
        pack.tag = "字".repeat(24);
        let imported = KnowledgePack::from_json(&serde_json::to_vec(&pack).unwrap()).unwrap();
        library.import(imported.clone(), true).unwrap();
        assert_eq!(library.packs[0].tag, imported.tag);
    }

    #[test]
    fn invalid_import_leaves_original_library_intact() {
        let mut library = crate::test_fixtures::regression_library();
        let original = library.clone();
        let mut invalid = library.packs[0].clone();
        invalid.cards[0].answer = " ".into();
        assert!(library.import(invalid, true).is_err());
        assert_eq!(library, original);
        let mut invalid = original.packs[0].clone();
        invalid.id = "../outside".into();
        assert!(invalid.validate().is_err());
        let mut unsupported = original.packs[0].clone();
        unsupported.schema_version = 2;
        assert!(unsupported.validate().is_err());
    }

    #[test]
    fn replacement_requires_confirmation_and_keeps_enabled_state() {
        let mut library = crate::test_fixtures::regression_library();
        let mut replacement = library.packs[0].clone();
        replacement.name = "更新后的题库".into();
        assert!(library.import(replacement.clone(), false).is_err());
        library.set_pack_enabled(&replacement.id, false).unwrap();
        library.import(replacement.clone(), true).unwrap();
        assert_eq!(library.packs.len(), 1);
        assert_eq!(library.packs[0].name, replacement.name);
        assert!(!library.is_enabled(&replacement.id));
    }
}
