use crate::knowledge::{AppResult, KnowledgePack, Library, MAX_PACK_BYTES, MAX_STATE_BYTES};
use crate::media::{self, MediaStore};
use crate::tr;
use base64::{Engine, engine::general_purpose::STANDARD};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

pub struct Store {
    directory: PathBuf,
    _lock: File,
}

impl Store {
    pub fn open(directory: PathBuf) -> AppResult<Self> {
        fs::create_dir_all(&directory)
            .map_err(|error| tr!("无法创建本地数据目录：{error}", error = error))?;
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(directory.join("try.lock"))
            .map_err(|error| tr!("无法打开本地数据锁：{error}", error = error))?;
        lock.try_lock().map_err(|error| {
            tr!(
                "无法独占本地数据。Try 可能已在运行，请从托盘打开主窗口。\n{error}",
                error = error
            )
        })?;
        Ok(Self {
            directory,
            _lock: lock,
        })
    }

    pub fn directory(&self) -> &Path {
        &self.directory
    }

    pub fn media(&self) -> MediaStore {
        MediaStore::new(&self.directory)
    }

    /// Embedded image bytes exist only during interchange, never in Library/state.json.
    pub fn import_pack(&self, path: &Path) -> AppResult<KnowledgePack> {
        let bytes = read_bounded(path, media::MAX_BUNDLE_BYTES)?;
        let mut document: serde_json::Value =
            serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
        let embedded = document.as_object_mut().and_then(|o| o.remove("media"));
        if embedded.is_none() && bytes.len() as u64 > MAX_PACK_BYTES {
            return Err(tr!("这个知识包超过 2 MiB，请拆分后再导出。").into());
        }
        let pack =
            KnowledgePack::from_json(&serde_json::to_vec(&document).map_err(|e| e.to_string())?)?;
        let images: BTreeMap<String, String> = embedded
            .map(serde_json::from_value)
            .transpose()
            .map_err(|e| e.to_string())?
            .unwrap_or_default();
        let referenced: BTreeSet<_> = pack
            .cards
            .iter()
            .filter_map(|c| c.prompt_image.clone())
            .collect();
        if images.len() > 128
            || images.len() != referenced.len()
            || images.keys().any(|id| !referenced.contains(id))
        {
            return Err(tr!("知识包包含多余或过多的图片资源。").into());
        }
        let mut staged = Vec::new();
        let mut total = 0;
        for id in &referenced {
            if let Some(encoded) = images.get(id) {
                if encoded.len() > (media::MAX_IMAGE_BYTES as usize).div_ceil(3) * 4 {
                    return Err(tr!("知识包图片资源过大，请拆分知识包。").into());
                }
                let png = STANDARD
                    .decode(encoded)
                    .map_err(|_| tr!("知识包图片编码无效。"))?;
                total += png.len();
                if total > media::MAX_BUNDLE_MEDIA_BYTES {
                    return Err(tr!("知识包图片资源过大，请拆分知识包。").into());
                }
                staged.push((id, png));
            } else {
                return Err(tr!("题面图片缺失，请重新添加或导入知识包。").into());
            }
        }
        for (id, png) in staged {
            self.media().install(id, &png)?;
        }
        Ok(pack)
    }

    pub fn export_portable_pack(&self, path: &Path, pack: &KnowledgePack) -> AppResult<()> {
        let referenced: BTreeSet<_> = pack
            .cards
            .iter()
            .filter_map(|c| c.prompt_image.clone())
            .collect();
        if referenced.is_empty() {
            return Self::export_pack(path, pack);
        }
        pack.validate()?;
        if serde_json::to_vec(pack).map_err(|e| e.to_string())?.len() as u64 > MAX_PACK_BYTES {
            return Err(tr!("这个知识包超过 2 MiB，请拆分后再导出。").into());
        }
        let mut images = BTreeMap::new();
        let mut total = 0;
        if referenced.len() > 128 {
            return Err(tr!("知识包包含多余或过多的图片资源。").into());
        }
        for id in referenced {
            let png = self.media().read(&id)?;
            total += png.len();
            if total > media::MAX_BUNDLE_MEDIA_BYTES {
                return Err(tr!("知识包图片资源过大，请拆分知识包。").into());
            }
            images.insert(id, STANDARD.encode(png));
        }
        let mut document = serde_json::to_value(pack).map_err(|e| e.to_string())?;
        document["media"] = serde_json::to_value(images).map_err(|e| e.to_string())?;
        let bytes = serde_json::to_vec(&document).map_err(|e| e.to_string())?;
        if bytes.len() as u64 > media::MAX_BUNDLE_BYTES {
            return Err(tr!("知识包图片资源过大，请拆分知识包。").into());
        }
        write_atomic(path, &bytes)
    }

    pub fn load(&self) -> AppResult<Library> {
        let path = self.directory.join("state.json");
        if !path.exists() {
            let library = Library::default();
            self.save(&library)?;
            log::info!(target: "try::storage", "event=state_initialized packs={}", library.packs.len());
            return Ok(library);
        }
        let bytes = read_bounded(&path, MAX_STATE_BYTES)?;
        let mut library: Library = serde_json::from_slice(&bytes).map_err(|error| {
            tr!(
                "本地数据无法读取，原文件已保留：{error}\n{p0}",
                p0 = path.display(),
                error = error
            )
        })?;
        library.validate()?;
        let ids_changed = library.ensure_card_ids();
        let modes_changed = library.migrate_legacy_answer_modes();
        let configs_changed = library.migrate_legacy_practice_configs();
        let pause_changed = if let Some(enabled) = library.settings.reminders_enabled.take() {
            if !enabled {
                library.settings.pause = crate::reminder_schedule::PracticePause::UntilResumed;
            }
            true
        } else if library
            .settings
            .pause
            .expired(chrono::Utc::now().timestamp())
        {
            library.settings.pause = crate::reminder_schedule::PracticePause::None;
            true
        } else {
            false
        };
        if ids_changed || modes_changed || configs_changed || pause_changed {
            self.save(&library)?;
            log::info!(target: "try::storage",
                "event=state_migrated ids={ids_changed} modes={modes_changed} configs={configs_changed} pause={pause_changed}"
            );
        }
        log::info!(target: "try::storage",
            "event=state_loaded packs={} enabled_cards={}",
            library.packs.len(),
            library.enabled_card_count()
        );
        Ok(library)
    }

    pub fn save(&self, library: &Library) -> AppResult<()> {
        library.validate()?;
        for pack in &library.packs {
            let ids: BTreeSet<_> = pack
                .cards
                .iter()
                .filter_map(|c| c.prompt_image.as_deref())
                .collect();
            if ids.len() > 128 {
                return Err(tr!("知识包包含多余或过多的图片资源。").into());
            }
            let mut total = 0;
            for id in ids {
                total += self.media().size(id)?;
            }
            if total > media::MAX_BUNDLE_MEDIA_BYTES as u64 {
                return Err(tr!("知识包图片资源过大，请拆分知识包。").into());
            }
        }
        let bytes = serde_json::to_vec_pretty(library).map_err(|error| error.to_string())?;
        if bytes.len() as u64 > MAX_STATE_BYTES {
            return Err(tr!("本地数据超过首版的 16 MiB 上限，请减少知识包。").into());
        }
        let result = write_atomic(&self.directory.join("state.json"), &bytes);
        match &result {
            Ok(()) => {
                log::debug!(target: "try::storage", "event=state_saved bytes={}", bytes.len())
            }
            Err(error) => {
                log::error!(target: "try::storage", "event=state_save_failed reason={error}")
            }
        }
        result
    }

    pub fn read_pack(path: &Path) -> AppResult<KnowledgePack> {
        let pack = KnowledgePack::from_json(&read_bounded(path, MAX_PACK_BYTES)?)?;
        log::info!(target: "try::storage", "event=pack_read cards={}", pack.cards.len());
        Ok(pack)
    }

    pub fn export_pack(path: &Path, pack: &KnowledgePack) -> AppResult<()> {
        pack.validate()?;
        let mut bytes = serde_json::to_vec_pretty(pack).map_err(|error| error.to_string())?;
        if bytes.len() as u64 > MAX_PACK_BYTES {
            bytes = serde_json::to_vec(pack).map_err(|error| error.to_string())?;
        }
        if bytes.len() as u64 > MAX_PACK_BYTES {
            return Err(tr!("这个知识包超过 2 MiB，请拆分后再导出。").into());
        }
        let result = write_atomic(path, &bytes);
        match &result {
            Ok(()) => log::info!(target: "try::storage",
                "event=pack_exported cards={} bytes={}",
                pack.cards.len(),
                bytes.len()
            ),
            Err(error) => {
                log::error!(target: "try::storage", "event=pack_export_failed reason={error}")
            }
        }
        result
    }
}

pub(crate) fn read_bounded(path: &Path, limit: u64) -> AppResult<Vec<u8>> {
    let file = File::open(path)
        .map_err(|error| tr!("无法打开 {p0}：{error}", p0 = path.display(), error = error))?;
    let mut bytes = Vec::new();
    file.take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| tr!("无法读取 {p0}：{error}", p0 = path.display(), error = error))?;
    if bytes.len() as u64 > limit {
        return Err(tr!("{p0} 超过允许的文件大小。", p0 = path.display()));
    }
    Ok(bytes)
}

pub(crate) fn write_atomic(path: &Path, bytes: &[u8]) -> AppResult<()> {
    let parent = path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut temporary = tempfile::NamedTempFile::new_in(parent)
        .map_err(|error| tr!("无法在目标目录创建临时文件：{error}", error = error))?;
    temporary
        .write_all(bytes)
        .map_err(|error| tr!("无法写入文件：{error}", error = error))?;
    temporary
        .as_file()
        .sync_all()
        .map_err(|error| tr!("无法保存文件：{error}", error = error))?;
    temporary.persist(path).map_err(|error| {
        tr!(
            "无法保存到 {p0}：{error}",
            p0 = path.display(),
            error = error
        )
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn media_pack_is_self_contained_and_state_keeps_only_resource_ids() {
        let source = tempfile::tempdir().unwrap();
        let target = tempfile::tempdir().unwrap();
        let store = Store::open(source.path().join("data")).unwrap();
        let image_path = source.path().join("source.png");
        image::DynamicImage::new_rgb8(40, 20)
            .save(&image_path)
            .unwrap();
        let normalized = media::normalize_file(&image_path).unwrap();
        store
            .media()
            .install(&normalized.id, &normalized.png)
            .unwrap();
        let mut library = Library::default();
        library.packs[0].cards[0].prompt_image = Some(normalized.id.clone());
        library.packs[0].cards[0].prompt.clear();
        library.packs[0].cards[0].prompt_latex = None;
        library.packs[0].cards[1].prompt_image = Some(normalized.id.clone());
        store.save(&library).unwrap();
        fs::remove_file(&image_path).unwrap();
        assert_eq!(store.load().unwrap().packs, library.packs);
        let raw_state = fs::read_to_string(store.directory().join("state.json")).unwrap();
        assert!(!raw_state.contains("data:image"));
        assert!(!raw_state.contains(&STANDARD.encode(&normalized.png)));
        let bundle = target.path().join("images.json");
        store
            .export_portable_pack(&bundle, &library.packs[0])
            .unwrap();
        let document: serde_json::Value =
            serde_json::from_slice(&fs::read(&bundle).unwrap()).unwrap();
        assert_eq!(document["media"].as_object().unwrap().len(), 1);
        let other = Store::open(target.path().join("fresh_data")).unwrap();
        let imported = other.import_pack(&bundle).unwrap();
        assert_eq!(imported, library.packs[0]);
        assert_eq!(other.media().read(&normalized.id).unwrap(), normalized.png);
        let before = fs::read(store.directory().join("state.json")).unwrap();
        let mut invalid = document.clone();
        invalid["media"][&normalized.id] = serde_json::json!(STANDARD.encode(b"corrupt"));
        fs::write(&bundle, serde_json::to_vec(&invalid).unwrap()).unwrap();
        assert!(store.import_pack(&bundle).is_err());
        assert_eq!(
            fs::read(store.directory().join("state.json")).unwrap(),
            before
        );
        assert_eq!(store.media().read(&normalized.id).unwrap(), normalized.png);
        invalid["cards"][0]["prompt_image"] = serde_json::json!("../../outside.png");
        fs::write(&bundle, serde_json::to_vec(&invalid).unwrap()).unwrap();
        assert!(other.import_pack(&bundle).is_err());
        drop(store);
        let reopened = Store::open(source.path().join("data")).unwrap();
        assert_eq!(reopened.load().unwrap().packs, library.packs);
    }

    #[test]
    fn deleting_questions_persists_and_empty_packs_export_and_reimport() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::open(directory.path().to_owned()).unwrap();
        let mut library = store.load().unwrap();
        let original = library.packs[0].clone();
        library.settings.enabled_pack_ids = vec![original.id.clone()];
        for card in &original.cards {
            library
                .delete_card(&original.id, card.id.as_deref().unwrap())
                .unwrap();
        }
        store.save(&library).unwrap();
        drop(store);
        let store = Store::open(directory.path().to_owned()).unwrap();
        assert_eq!(store.load().unwrap(), library);
        let exported = directory.path().join("empty_pack.json");
        Store::export_pack(&exported, &library.packs[0]).unwrap();
        let imported = Store::read_pack(&exported).unwrap();
        assert_eq!(imported, library.packs[0]);
        library.import(imported, true).unwrap();
        library
            .save_card(&original.id, None, original.cards[0].clone())
            .unwrap();
        store.save(&library).unwrap();
        assert_eq!(store.load().unwrap(), library);
        assert_eq!(library.enabled_card_count(), 1);
    }

    #[test]
    fn deleting_the_last_pack_survives_reopening_and_allows_reimport() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::open(directory.path().to_owned()).unwrap();
        let mut library = store.load().unwrap();
        let original = library.packs[0].clone();
        library.settings.interval_minutes = 47;
        let ids: Vec<_> = library.packs.iter().map(|pack| pack.id.clone()).collect();
        for id in ids {
            library.delete_pack(&id).unwrap();
        }
        store.save(&library).unwrap();
        drop(store);

        let store = Store::open(directory.path().to_owned()).unwrap();
        let mut reloaded = store.load().unwrap();
        assert_eq!(reloaded, library);
        assert!(reloaded.packs.is_empty());
        assert_eq!(reloaded.enabled_card_count(), 0);
        assert_eq!(reloaded.settings.interval_minutes, 47);
        assert!(crate::practice::select_question(&reloaded, None, &mut rand::rng()).is_none());

        reloaded.import(original.clone(), false).unwrap();
        store.save(&reloaded).unwrap();
        assert_eq!(store.load().unwrap().packs, [original]);
        assert!(reloaded.enabled_card_count() > 0);
    }

    #[test]
    fn fixed_overrides_migrate_to_effective_question_modes_once() {
        use crate::knowledge::{AnswerMode, PracticeConfig};
        for (config, expected) in [
            (PracticeConfig::Custom, AnswerMode::TrueFalse),
            (PracticeConfig::Input, AnswerMode::Input),
            (PracticeConfig::MultipleChoice, AnswerMode::MultipleChoice),
            (PracticeConfig::TrueFalse, AnswerMode::TrueFalse),
        ] {
            let directory = tempfile::tempdir().unwrap();
            let mut old = Library::default();
            let pack = old.packs[0].clone();
            let id = pack.cards[0].id.as_ref().unwrap();
            old.set_all_answer_modes(&pack.id, AnswerMode::TrueFalse)
                .unwrap();
            old.set_card_enabled(&pack.id, id, false).unwrap();
            old.settings.interval_minutes = 49;
            old.settings
                .pack_practice_configs
                .insert(pack.id.clone(), config);
            let store = Store::open(directory.path().to_owned()).unwrap();
            store.save(&old).unwrap();
            let mut migrated = store.load().unwrap();
            assert!(migrated.settings.pack_practice_configs.is_empty());
            assert_eq!(migrated.uniform_answer_mode(&pack.id), Some(expected));
            assert_eq!(migrated.packs, old.packs);
            assert_eq!(
                migrated.settings.excluded_card_ids,
                old.settings.excluded_card_ids
            );
            assert_eq!(migrated.settings.interval_minutes, 49);
            let changed = if expected == AnswerMode::Input {
                AnswerMode::TrueFalse
            } else {
                AnswerMode::Input
            };
            migrated
                .set_card_answer_mode(&pack.id, id, changed)
                .unwrap();
            store.save(&migrated).unwrap();
            assert_eq!(store.load().unwrap(), migrated);
        }
    }

    #[test]
    fn pack_metadata_survives_restart_and_export_without_changing_identity() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::open(directory.path().to_owned()).unwrap();
        let mut library = store.load().unwrap();
        let original = library.packs[0].clone();
        let settings = library.settings.clone();
        library.rename_pack(&original.id, "  常见角  ").unwrap();
        library.set_pack_tag(&original.id, "  数学  ").unwrap();
        assert_eq!(library.packs[0].tag, "数学");
        library
            .set_pack_description(&original.id, "先记住常见角。\n偶尔看一眼。")
            .unwrap();
        let valid = library.clone();
        assert!(library.rename_pack(&original.id, "  ").is_err());
        for tag in ["长".repeat(25), "数学\n化学".into(), "数学\t".into()] {
            assert!(library.set_pack_tag(&original.id, &tag).is_err());
        }
        assert!(
            library
                .set_pack_description(&original.id, &"长".repeat(2001))
                .is_err()
        );
        assert_eq!(library, valid);
        store.save(&library).unwrap();
        assert_eq!(store.load().unwrap(), library);
        let path = directory.path().join("with_description.json");
        Store::export_pack(&path, &library.packs[0]).unwrap();
        assert_eq!(Store::read_pack(&path).unwrap(), library.packs[0]);
        assert_eq!(library.packs[0].id, original.id);
        assert_eq!(library.packs[0].cards, original.cards);
        assert_eq!(library.settings, settings);
    }

    #[test]
    fn legacy_data_keeps_packs_and_persists_tag_visibility_separately_from_exports() {
        let directory = tempfile::tempdir().unwrap();
        let old = Library::default();
        let mut json = serde_json::to_value(&old).unwrap();
        for pack in json["packs"].as_array_mut().unwrap() {
            pack.as_object_mut().unwrap().remove("tag");
        }
        json["settings"]
            .as_object_mut()
            .unwrap()
            .remove("show_pack_tags");
        fs::write(
            directory.path().join("state.json"),
            serde_json::to_vec(&json).unwrap(),
        )
        .unwrap();
        let store = Store::open(directory.path().to_owned()).unwrap();
        let mut restored = store.load().unwrap();
        assert!(restored.settings.show_pack_tags);
        assert!(restored.packs.iter().all(|pack| pack.tag.is_empty()));
        for (previous, current) in old.packs.iter().zip(&restored.packs) {
            assert_eq!(previous.cards, current.cards);
        }
        let id = restored.packs[0].id.clone();
        restored.set_pack_tag(&id, "化学").unwrap();
        restored.settings.show_pack_tags = false;
        store.save(&restored).unwrap();
        let loaded = store.load().unwrap();
        assert!(!loaded.settings.show_pack_tags);
        let path = directory.path().join("tagged.json");
        Store::export_pack(&path, &loaded.packs[0]).unwrap();
        assert_eq!(Store::read_pack(&path).unwrap().tag, "化学");
        let exported: serde_json::Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
        assert!(exported.get("show_pack_tags").is_none());
        restored.set_pack_tag(&id, "").unwrap();
        store.save(&restored).unwrap();
        assert!(store.load().unwrap().packs[0].tag.is_empty());
    }

    #[test]
    fn legacy_pack_modes_migrate_once_to_existing_questions() {
        use crate::knowledge::AnswerMode;
        let directory = tempfile::tempdir().unwrap();
        let mut old = Library::default();
        old.settings.enabled_pack_ids.clear();
        old.settings
            .answer_modes
            .insert(old.packs[0].id.clone(), AnswerMode::TrueFalse);
        let original_cards = old.packs[0].cards.clone();
        let mut json = serde_json::to_value(&old).unwrap();
        json["settings"]
            .as_object_mut()
            .unwrap()
            .remove("card_answer_modes");
        fs::write(
            directory.path().join("state.json"),
            serde_json::to_vec(&json).unwrap(),
        )
        .unwrap();
        let store = Store::open(directory.path().to_owned()).unwrap();
        let migrated = store.load().unwrap();
        assert!(migrated.settings.answer_modes.is_empty());
        assert_eq!(migrated.packs[0].cards, original_cards);
        assert_eq!(
            migrated.uniform_answer_mode(&migrated.packs[0].id),
            Some(AnswerMode::TrueFalse)
        );
        assert_eq!(migrated.enabled_card_count(), 0);
        assert_eq!(
            migrated.settings.interval_minutes,
            old.settings.interval_minutes
        );
        assert_eq!(store.load().unwrap(), migrated);
        let mut mixed = migrated;
        let pack_id = mixed.packs[0].id.clone();
        let card_id = mixed.packs[0].cards[0].id.clone().unwrap();
        mixed
            .set_card_answer_mode(&pack_id, &card_id, AnswerMode::Input)
            .unwrap();
        store.save(&mixed).unwrap();
        drop(store);
        let reopened = Store::open(directory.path().to_owned()).unwrap();
        assert_eq!(
            reopened.load().unwrap(),
            mixed,
            "Restart must not reapply the old pack-wide mode"
        );
    }

    #[test]
    fn legacy_state_migrates_ids_without_enabling_disabled_packs() {
        let directory = tempfile::tempdir().unwrap();
        let mut old = Library::default();
        old.settings.enabled_pack_ids.clear();
        old.settings.interval_minutes = 47;
        for pack in &mut old.packs {
            for card in &mut pack.cards {
                card.id = None;
            }
        }
        let mut old_json = serde_json::to_value(&old).unwrap();
        old_json["settings"]
            .as_object_mut()
            .unwrap()
            .remove("excluded_card_ids");
        fs::write(
            directory.path().join("state.json"),
            serde_json::to_vec(&old_json).unwrap(),
        )
        .unwrap();
        let store = Store::open(directory.path().to_owned()).unwrap();
        let migrated = store.load().unwrap();
        assert_eq!(migrated.enabled_card_count(), 0);
        assert_eq!(migrated.settings, old.settings);
        assert!(migrated.packs[0].cards.iter().all(|card| card.id.is_some()));
        assert_eq!(
            store.load().unwrap(),
            migrated,
            "Identifiers are saved once"
        );
    }

    #[test]
    fn edited_question_and_partial_selection_survive_restart_and_export() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::open(directory.path().to_owned()).unwrap();
        let mut library = store.load().unwrap();
        library.settings.enabled_pack_ids.clear();
        let pack_id = library.packs[0].id.clone();
        let card_id = library.packs[0].cards[0].id.clone().unwrap();
        library.set_pack_enabled(&pack_id, false).unwrap();
        library.set_card_enabled(&pack_id, &card_id, true).unwrap();
        let mut edited = library.packs[0].cards[0].clone();
        edited.prompt = "Edited 中文 question".into();
        library.save_card(&pack_id, Some(&card_id), edited).unwrap();
        library
            .set_card_answer_mode(&pack_id, &card_id, crate::knowledge::AnswerMode::TrueFalse)
            .unwrap();
        store.save(&library).unwrap();
        drop(store);
        let reopened = Store::open(directory.path().to_owned()).unwrap();
        let restored = reopened.load().unwrap();
        assert_eq!(restored, library);
        assert_eq!(
            restored.card_answer_mode(&pack_id, &card_id),
            crate::knowledge::AnswerMode::TrueFalse
        );
        assert_eq!(restored.enabled_card_count(), 1);
        let path = directory.path().join("edited_export.json");
        Store::export_pack(&path, &library.packs[0]).unwrap();
        assert_eq!(Store::read_pack(&path).unwrap(), library.packs[0]);
    }

    #[test]
    fn display_durations_migrate_independently_and_survive_restart() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::open(directory.path().to_owned()).unwrap();
        let mut original = store.load().unwrap();
        original.settings.feedback_seconds = 17;
        let mut legacy = serde_json::to_value(&original).unwrap();
        legacy["settings"]
            .as_object_mut()
            .unwrap()
            .remove("knowledge_seconds");
        let path = directory.path().join("state.json");
        let legacy_bytes = serde_json::to_vec(&legacy).unwrap();
        fs::write(&path, &legacy_bytes).unwrap();

        let mut library = store.load().unwrap();
        assert_eq!(library.settings.feedback_seconds, 17);
        assert_eq!(library.settings.knowledge_seconds, 17);
        assert_eq!(fs::read(&path).unwrap(), legacy_bytes);
        assert_eq!(library.packs, original.packs);

        library.settings.feedback_seconds = 2;
        store.save(&library).unwrap();
        assert_eq!(store.load().unwrap().settings.knowledge_seconds, 17);
        library.settings.knowledge_seconds = 25;
        store.save(&library).unwrap();
        drop(store);
        let reopened = Store::open(directory.path().to_owned()).unwrap();
        assert_eq!(reopened.load().unwrap(), library);

        let saved = fs::read(&path).unwrap();
        for (feedback, knowledge) in [(0, 25), (61, 25), (2, 0), (2, 61)] {
            let mut invalid = library.clone();
            invalid.settings.feedback_seconds = feedback;
            invalid.settings.knowledge_seconds = knowledge;
            assert!(reopened.save(&invalid).is_err());
            assert_eq!(fs::read(&path).unwrap(), saved);
        }
        for invalid_value in [serde_json::Value::Null, 0.into(), 61.into(), (-1).into()] {
            let mut invalid = serde_json::to_value(&library).unwrap();
            invalid["settings"]["knowledge_seconds"] = invalid_value;
            fs::write(&path, serde_json::to_vec(&invalid).unwrap()).unwrap();
            assert!(reopened.load().is_err(), "Invalid values must not migrate");
        }
    }

    #[test]
    fn startup_display_preferences_survive_restart() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::open(directory.path().to_owned()).unwrap();
        let mut legacy = serde_json::to_value(store.load().unwrap()).unwrap();
        legacy["settings"]["feedback_seconds"] = 1.into();
        legacy["settings"]["knowledge_seconds"] = 15.into();
        legacy["settings"]
            .as_object_mut()
            .unwrap()
            .remove("knowledge_only");
        let path = directory.path().join("state.json");
        let legacy_bytes = serde_json::to_vec(&legacy).unwrap();
        fs::write(&path, &legacy_bytes).unwrap();
        let previous = store.load().unwrap();
        assert_eq!(previous.settings.feedback_seconds, 1);
        assert_eq!(previous.settings.knowledge_seconds, 15);
        assert_eq!(fs::read(&path).unwrap(), legacy_bytes);

        let mut preferences = serde_json::to_value(&previous).unwrap();
        preferences["settings"]["knowledge_only"] = true.into();
        fs::write(&path, serde_json::to_vec(&preferences).unwrap()).unwrap();
        let knowledge = store
            .load()
            .expect("The saved knowledge-display preference must be restored");
        store.save(&knowledge).unwrap();
        drop(store);

        let store = Store::open(directory.path().to_owned()).unwrap();
        let restored = store.load().unwrap();
        assert_eq!(restored, knowledge);
        assert_eq!(restored.settings.feedback_seconds, 1);
        assert_eq!(restored.settings.knowledge_seconds, 15);
        assert_eq!(
            serde_json::to_value(&restored).unwrap()["settings"]["knowledge_only"],
            true
        );
        assert_eq!(restored.packs, previous.packs);

        let mut preferences = serde_json::to_value(&restored).unwrap();
        preferences["settings"]["knowledge_only"] = false.into();
        let answer = serde_json::from_value(preferences).unwrap();
        store.save(&answer).unwrap();
        drop(store);
        let restored = Store::open(directory.path().to_owned())
            .unwrap()
            .load()
            .unwrap();
        assert_eq!(restored, answer);
        assert_eq!(restored.settings.feedback_seconds, 1);
        assert_eq!(restored.settings.knowledge_seconds, 15);
    }

    #[test]
    fn settings_and_imported_content_survive_source_removal_and_restart() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::open(directory.path().to_owned()).unwrap();
        let mut library = store.load().unwrap();
        library.settings.interval_minutes = 45;
        library.settings.feedback_seconds = 1;
        library.settings.knowledge_seconds = 30;
        library.settings.pause = crate::reminder_schedule::PracticePause::from_minutes(
            180,
            chrono::Utc::now().timestamp(),
        )
        .unwrap();
        library.settings.quiet_hours =
            crate::reminder_schedule::QuietHours::from_text(true, "22:30", "07:15").unwrap();
        library.settings.popup_corner = crate::window_position::PopupCorner::TopLeft;
        library.settings.popup_position = Some(crate::window_position::SavedPopupPosition {
            monitor: "left_display".into(),
            x: 80.0,
            y: 120.0,
        });
        let mut imported = library.packs[0].clone();
        imported.id = "my_math".into();
        let source_directory = tempfile::tempdir().unwrap();
        let source = source_directory.path().join("original.json");
        Store::export_pack(&source, &imported).unwrap();
        library
            .import(Store::read_pack(&source).unwrap(), false)
            .unwrap();
        store.save(&library).unwrap();
        drop(store);

        let moved = source_directory.path().join("renamed.json");
        fs::rename(&source, &moved).unwrap();
        fs::remove_file(&moved).unwrap();
        drop(source_directory);
        let reopened = Store::open(directory.path().to_owned()).unwrap();
        assert_eq!(reopened.load().unwrap(), library);
    }

    #[test]
    fn starter_content_is_created_once_without_expanding_existing_personal_libraries() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::open(directory.path().to_owned()).unwrap();
        let starter = store.load().unwrap();
        assert_eq!(starter, Library::default());
        assert_eq!(store.load().unwrap(), starter);
        drop(store);

        let store = Store::open(directory.path().to_owned()).unwrap();
        let mut existing = crate::test_fixtures::regression_library();
        existing.settings.interval_minutes = 47;
        existing.packs[0].name = "我的题库".into();
        store.save(&existing).unwrap();
        let before = fs::read(directory.path().join("state.json")).unwrap();
        drop(store);
        let store = Store::open(directory.path().to_owned()).unwrap();
        assert_eq!(store.load().unwrap(), existing);
        assert_eq!(
            fs::read(directory.path().join("state.json")).unwrap(),
            before
        );
    }

    #[test]
    fn old_disabled_reminders_become_manual_pause_and_expired_pause_is_cleared() {
        use crate::reminder_schedule::PracticePause;
        for enabled in [true, false] {
            let directory = tempfile::tempdir().unwrap();
            let store = Store::open(directory.path().to_owned()).unwrap();
            let original = Library::default();
            let mut old = serde_json::to_value(&original).unwrap();
            old["settings"]["reminders_enabled"] = enabled.into();
            old["settings"].as_object_mut().unwrap().remove("pause");
            fs::write(
                directory.path().join("state.json"),
                serde_json::to_vec(&old).unwrap(),
            )
            .unwrap();
            let restored = store.load().unwrap();
            let mut expected = original;
            expected.settings.pause = if enabled {
                PracticePause::None
            } else {
                PracticePause::UntilResumed
            };
            assert_eq!(restored, expected);
            assert_eq!(store.load().unwrap(), expected);
            assert!(
                serde_json::to_value(&expected).unwrap()["settings"]
                    .get("reminders_enabled")
                    .is_none()
            );
        }
        let directory = tempfile::tempdir().unwrap();
        let store = Store::open(directory.path().to_owned()).unwrap();
        let mut library = Library::default();
        library.settings.pause = PracticePause::Timed {
            until: chrono::Utc::now().timestamp() - 1,
            minutes: 180,
        };
        store.save(&library).unwrap();
        library.settings.pause = PracticePause::None;
        assert_eq!(store.load().unwrap(), library);
        assert_eq!(store.load().unwrap(), library);
        library.settings.pause = PracticePause::UntilResumed;
        store.save(&library).unwrap();
        assert_eq!(store.load().unwrap(), library);
    }

    #[test]
    fn rejected_save_does_not_overwrite_good_data() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::open(directory.path().to_owned()).unwrap();
        let mut invalid = store.load().unwrap();
        let original = fs::read(directory.path().join("state.json")).unwrap();
        invalid.settings.interval_minutes = 0;
        assert!(store.save(&invalid).is_err());
        assert_eq!(
            fs::read(directory.path().join("state.json")).unwrap(),
            original
        );
        invalid.settings.interval_minutes = 30;
        invalid.settings.quiet_hours.start_minute = 1440;
        assert!(store.save(&invalid).is_err());
        assert_eq!(
            fs::read(directory.path().join("state.json")).unwrap(),
            original
        );
    }

    #[test]
    fn export_can_be_reimported_and_replaces_an_existing_file() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("export.json");
        fs::write(&path, "old content").unwrap();
        let pack = Library::default().packs.remove(0);
        Store::export_pack(&path, &pack).unwrap();
        assert_eq!(Store::read_pack(&path).unwrap(), pack);
    }

    #[test]
    fn near_limit_pack_exports_to_a_reimportable_file() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("large_export.json");
        let mut pack = Library::default().packs.remove(0);
        pack.cards = vec![
            crate::knowledge::Card {
                prompt_image: None,
                id: None,
                prompt: "q".repeat(1000),
                answer: "a".repeat(1000),
                aliases: vec![],
                prompt_latex: None,
                answer_latex: None,
                distractors: vec![],
                true_false: None,
            };
            1030
        ];
        let compact = serde_json::to_vec(&pack).unwrap();
        assert!(compact.len() as u64 <= MAX_PACK_BYTES);
        assert!(serde_json::to_vec_pretty(&pack).unwrap().len() as u64 > MAX_PACK_BYTES);
        let imported = KnowledgePack::from_json(&compact).unwrap();
        Store::export_pack(&path, &imported).unwrap();
        assert!(fs::metadata(&path).unwrap().len() <= MAX_PACK_BYTES);
        assert_eq!(Store::read_pack(&path).unwrap(), imported);
    }

    #[test]
    fn corrupt_existing_state_is_reported_and_preserved() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("state.json");
        fs::write(&path, "broken json").unwrap();
        let store = Store::open(directory.path().to_owned()).unwrap();
        assert!(store.load().is_err());
        assert_eq!(fs::read_to_string(path).unwrap(), "broken json");
    }

    #[test]
    fn a_second_writer_cannot_open_the_same_profile() {
        let directory = tempfile::tempdir().unwrap();
        let first = Store::open(directory.path().to_owned()).unwrap();
        assert!(Store::open(directory.path().to_owned()).is_err());
        drop(first);
        assert!(Store::open(directory.path().to_owned()).is_ok());
    }
}
