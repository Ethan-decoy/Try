//! Host presentation for SMI actions. Editing semantics stay in SMI.
use crate::MathKey;
use slint::{ModelRc, Rgba8Pixel, SharedPixelBuffer, VecModel};
use slint_math_input::catalog::CatalogEntry;
use r#try::tr;

struct Category {
    name: &'static str,
    sources: &'static [&'static str],
}

const CATEGORIES: &[Category] = &[
    Category {
        name: "基本结构",
        sources: &["基本结构", "重音与注记"],
    },
    Category {
        name: "常用函数",
        sources: &["常用函数与数论"],
    },
    Category {
        name: "微积分",
        sources: &["微积分"],
    },
    Category {
        name: "三角函数",
        sources: &["三角函数", "双曲与反双曲函数"],
    },
    Category {
        name: "统计与概率",
        sources: &["统计与列表", "概率与分布"],
    },
    Category {
        name: "矩阵与线性代数",
        sources: &["矩阵与分段", "线性代数函数"],
    },
    Category {
        name: "希腊字母",
        sources: &["希腊字母"],
    },
    Category {
        name: "关系与集合",
        sources: &["关系与集合"],
    },
    Category {
        name: "符号与常数",
        sources: &["箭头与运算", "常数与字母集"],
    },
];

// Match SMI's curated mouse catalog; formatting and interchangeable aliases
// remain available through typing, LaTeX and the component's public API.
const CORE_ONLY: &[&str] = &[
    "bold",
    "blackboard",
    "calligraphic",
    "fraktur",
    "italic",
    "sans-serif",
    "monospace",
    "underline",
    "fn-sign",
    "fn-factorial",
    "fn-asinh",
    "fn-acosh",
    "fn-atanh",
    "fn-asech",
    "fn-acsch",
    "fn-acoth",
];

pub fn categories() -> ModelRc<slint::SharedString> {
    ModelRc::new(VecModel::from(
        CATEGORIES
            .iter()
            .map(|category| r#try::i18n::text(category.name).into())
            .collect::<Vec<_>>(),
    ))
}

pub fn catalog(index: i32) -> ModelRc<MathKey> {
    let category = &CATEGORIES[index.clamp(0, CATEGORIES.len() as i32 - 1) as usize];
    ModelRc::new(VecModel::from(
        category
            .sources
            .iter()
            .flat_map(|source| {
                slint_math_input::catalog::entries()
                    .iter()
                    .filter(move |entry| entry.category == *source)
            })
            .filter(|entry| !CORE_ONLY.contains(&entry.id.as_str()))
            .map(catalog_key)
            .collect::<Vec<_>>(),
    ))
}

pub fn common_keys() -> ModelRc<MathKey> {
    const STRUCTURES: &[&str] = &[
        "fraction",
        "power",
        "subscript",
        "log-base",
        "root",
        "indexed-root",
        "derivative",
        "partial-derivative",
        "integral",
        "sum",
        "product",
        "lim",
    ];
    ModelRc::new(VecModel::from(
        STRUCTURES
            .iter()
            .map(|id| {
                catalog_key(
                    slint_math_input::catalog::entry(id)
                        .expect("structural action exists in pinned SMI"),
                )
            })
            .collect::<Vec<_>>(),
    ))
}

fn catalog_key(entry: &CatalogEntry) -> MathKey {
    let label = translated_label(&entry.label);
    let hint = match entry.id.as_str() {
        "fraction" => tr!("分数：分子 → 分母"),
        "root" => tr!("平方根"),
        "indexed-root" => tr!("根式：次数 → 根号内"),
        "power" => tr!("上标"),
        "subscript" => tr!("下标"),
        "square" => tr!("平方"),
        "log-base" | "fn-log" => tr!("对数：填写底数，Tab 进入真数"),
        "integral" => tr!("积分：下限 → 上限 → 被积函数"),
        _ => "",
    };
    MathKey {
        token: format!("catalog:{}", entry.id).into(),
        hint: if hint.is_empty()
            && entry
                .label
                .chars()
                .any(|c| matches!(c, '\u{2e80}'..='\u{9fff}'))
        {
            label.clone().into()
        } else if hint.is_empty() {
            tr!("插入 {symbol}", symbol = label.as_str()).into()
        } else {
            hint.into()
        },
        image: catalog_image(entry),
        label: label.into(),
    }
}

fn translated_label(label: &str) -> String {
    const TEXT: &[&str] = &[
        "多行对齐",
        "矩阵",
        "列向量",
        "行向量",
        "上方注记",
        "上括注",
        "文字",
        "下方注记",
        "下括注",
        "向量箭头",
        "增加列",
        "增加行",
        "减少列",
        "减少行",
    ];
    if let Some(delimiter) = label.strip_prefix("闭合 ") {
        return tr!("闭合 {delimiter}", delimiter = delimiter);
    }
    if let Some(rows) = label
        .strip_prefix("分段 ")
        .and_then(|s| s.strip_suffix(" 行"))
    {
        return tr!("分段 {rows} 行", rows = rows);
    }
    TEXT.iter()
        .find(|&&text| label == text)
        .map(|&text| r#try::i18n::text(text).to_owned())
        .unwrap_or_else(|| label.to_owned())
}

fn catalog_image(entry: &CatalogEntry) -> slint::Image {
    if entry
        .label
        .chars()
        .any(|c| matches!(c, '\u{2e80}'..='\u{9fff}' | '\u{f900}'..='\u{faff}'))
    {
        return slint::Image::default();
    }
    let mut editor = slint_math_input::Editor::new();
    editor.font_size = 17.0;
    let latex = match entry.id.as_str() {
        "binomial" => Some(r"\binom{n}{k}"),
        "square" => Some(r"a^{2}"),
        "power" => Some(r"a^{b}"),
        "subscript" => Some(r"a_{n}"),
        "indexed-root" => Some(r"\sqrt[n]{a}"),
        "log-base" | "fn-log" => Some(r"\log_{a}\left(x\right)"),
        "integral" => Some(r"\int_{a}^{b}"),
        "dot" => Some(r"\dot{a}"),
        _ => None,
    };
    let inserted = match latex {
        Some(latex) => editor.import_latex(latex),
        None => editor.apply(slint_math_input::Action::Insert(entry.label.clone())),
    };
    inserted
        .and_then(|()| editor.render(2.0))
        .map(|rendered| {
            tight_image(
                &rendered.pixels,
                rendered.pixel_width,
                rendered.pixel_height,
            )
        })
        .unwrap_or_default()
}

fn tight_image(pixels: &[u8], width: u32, height: u32) -> slint::Image {
    let (mut left, mut top, mut right, mut bottom) = (width, height, 0, 0);
    for (index, pixel) in pixels.chunks_exact(4).enumerate() {
        if pixel[3] == 0 {
            continue;
        }
        let (x, y) = (index as u32 % width, index as u32 / width);
        left = left.min(x);
        top = top.min(y);
        right = right.max(x + 1);
        bottom = bottom.max(y + 1);
    }
    if left >= right || top >= bottom {
        return slint::Image::default();
    }
    // One logical pixel around the ink keeps antialiased edges intact. The
    // editor's 12px outer padding does not belong in a small symbol button.
    left = left.saturating_sub(2);
    top = top.saturating_sub(2);
    right = (right + 2).min(width);
    bottom = (bottom + 2).min(height);
    let mut cropped = Vec::with_capacity(((right - left) * (bottom - top) * 4) as usize);
    for y in top..bottom {
        cropped.extend_from_slice(
            &pixels[((y * width + left) * 4) as usize..((y * width + right) * 4) as usize],
        );
    }
    slint::Image::from_rgba8_premultiplied(SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(
        &cropped,
        right - left,
        bottom - top,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use slint::Model;

    #[test]
    fn first_page_contains_structures_instead_of_a_physical_keyboard() {
        let keys = common_keys();
        assert_eq!(keys.row_count(), 12);
        for key in keys.iter() {
            let id = key
                .token
                .strip_prefix("catalog:")
                .expect("structural action");
            assert!(matches!(
                slint_math_input::catalog::entry(id).unwrap().action,
                slint_math_input::Action::Template(_)
            ));
        }
        assert!(keys.iter().any(|key| key.token == "catalog:log-base"));
    }

    #[test]
    fn category_switches_start_at_the_top_and_common_keys_dispatch_their_actions() {
        use super::super::test_support::{click, paint, software_window};
        use crate::{MathInputState, PracticeWindow};
        use slint::{ComponentHandle, LogicalPosition, PhysicalSize};
        use std::{cell::RefCell, rc::Rc};
        let adapter = software_window();
        let window = PracticeWindow::new().unwrap();
        let tokens = Rc::new(RefCell::new(Vec::new()));
        let ui = window.global::<MathInputState>();
        ui.set_practice(true);
        ui.set_open(true);
        ui.set_keypad_expanded(true);
        ui.set_categories(categories());
        ui.set_catalog(catalog(0));
        ui.set_common_keys(common_keys());
        {
            let tokens = tokens.clone();
            ui.on_keypad(move |token, _| tokens.borrow_mut().push(token.to_string()));
        }
        {
            let weak = window.as_weak();
            ui.on_choose_category(move |index| {
                let window = weak.unwrap();
                let ui = window.global::<MathInputState>();
                ui.set_category(index);
                ui.set_catalog(catalog(index));
            });
        }
        window.show().unwrap();
        adapter.set_size(PhysicalSize::new(320, 560));
        paint(&adapter);
        click(
            &window,
            &adapter,
            ui.get_keypad_left() + ui.get_keypad_width() / 8.0,
            ui.get_keypad_top() + 16.0,
        );
        assert_eq!(tokens.borrow().last().unwrap(), "catalog:fraction");
        click(
            &window,
            &adapter,
            ui.get_keypad_left() + ui.get_keypad_width() * 3.0 / 4.0,
            ui.get_keypad_top() - 22.0,
        );
        ui.invoke_choose_category(1);
        paint(&adapter);
        window
            .window()
            .dispatch_event(slint::platform::WindowEvent::PointerScrolled {
                position: LogicalPosition::new(
                    ui.get_keypad_left() + 30.0,
                    ui.get_keypad_top() + 70.0,
                ),
                delta_x: 0.0,
                delta_y: -600.0,
            });
        paint(&adapter);
        ui.invoke_choose_category(2);
        paint(&adapter);
        click(
            &window,
            &adapter,
            ui.get_keypad_left() + ui.get_keypad_width() / 6.0,
            ui.get_keypad_top() + 18.0,
        );
        assert_eq!(tokens.borrow().last().unwrap(), "catalog:lim");
        click(
            &window,
            &adapter,
            ui.get_keypad_left() + ui.get_keypad_width() / 6.0,
            ui.get_keypad_top() - 60.0,
        );
        click(
            &window,
            &adapter,
            ui.get_keypad_left() + ui.get_keypad_width() * 1.5 / 4.0,
            ui.get_keypad_top() + 16.0,
        );
        assert_eq!(tokens.borrow().last().unwrap(), "catalog:power");
        window.hide().unwrap();
    }

    #[test]
    fn catalog_groups_cover_notation_without_formatting_or_duplicate_aliases() {
        let mut shown = std::collections::BTreeSet::new();
        for group in 0..CATEGORIES.len() {
            for key in catalog(group as i32).iter() {
                assert!(
                    shown.insert(key.token.to_string()),
                    "duplicate {}",
                    key.token
                );
            }
        }
        for entry in slint_math_input::catalog::entries() {
            assert_eq!(
                shown.contains(&format!("catalog:{}", entry.id)),
                !CORE_ONLY.contains(&entry.id.as_str()),
                "{}",
                entry.id
            );
        }
        for token in [
            "catalog:fraction",
            "catalog:power",
            "catalog:subscript",
            "catalog:log-base",
            "catalog:indexed-root",
        ] {
            assert!(
                common_keys().iter().any(|key| key.token == token),
                "{token}"
            );
        }
        let first_matrix_key = catalog(5).row_data(0).unwrap();
        let entry = slint_math_input::catalog::entry(
            first_matrix_key.token.strip_prefix("catalog:").unwrap(),
        )
        .unwrap();
        assert!(
            matches!(
                entry.action,
                slint_math_input::Action::Template(
                    slint_math_input::Template::StyledMatrix { .. }
                        | slint_math_input::Template::Matrix { .. }
                )
            ),
            "matrix construction comes before linear-algebra functions"
        );
    }

    #[test]
    fn captions_keep_mathematical_structure_without_editor_padding() {
        for (id, source) in [
            ("binomial", r"\binom{n}{k}"),
            ("square", r"a^{2}"),
            ("power", r"a^{b}"),
            ("subscript", r"a_{n}"),
            ("indexed-root", r"\sqrt[n]{a}"),
            ("log-base", r"\log_{a}\left(x\right)"),
            ("integral", r"\int_{a}^{b}"),
            ("dot", r"\dot{a}"),
        ] {
            let image = catalog_image(slint_math_input::catalog::entry(id).unwrap());
            let mut editor = slint_math_input::Editor::new();
            editor.font_size = 17.0;
            editor.import_latex(source).unwrap();
            let full = editor.render(2.0).unwrap();
            assert!(image.size().width > 0 && image.size().height > 0, "{id}");
            assert!(
                image.size().width < full.pixel_width && image.size().height < full.pixel_height,
                "{id}: remove the editor padding"
            );
        }
    }

    #[test]
    fn english_keys_and_hints_do_not_leave_chinese_labels_in_the_menu() {
        r#try::i18n::activate(r#try::i18n::InterfaceLanguage::English);
        for group in 0..CATEGORIES.len() {
            for key in catalog(group as i32).iter() {
                assert!(
                    !key.label
                        .chars()
                        .chain(key.hint.chars())
                        .any(|c| matches!(c, '\u{2e80}'..='\u{9fff}')),
                    "{}: {} / {}",
                    key.token,
                    key.label,
                    key.hint
                );
            }
        }
        r#try::i18n::activate(r#try::i18n::InterfaceLanguage::Chinese);
    }

    #[test]
    fn complete_catalog_samples_remain_usable_in_try() {
        let mut checked = 0;
        for entry in slint_math_input::catalog::entries() {
            let mut editor = slint_math_input::Editor::new();
            editor
                .apply(slint_math_input::Action::Insert("x".into()))
                .unwrap();
            editor.apply(slint_math_input::Action::SelectAll).unwrap();
            if editor.insert_catalog(&entry.id).is_ok()
                && let Ok(latex) = editor.latex()
            {
                r#try::formula::validate(&latex)
                    .unwrap_or_else(|error| panic!("{}: {latex}: {error}", entry.id));
                checked += 1;
            }
        }
        assert!(checked > 250);
    }
}
