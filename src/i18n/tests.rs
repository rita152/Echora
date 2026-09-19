use super::*;

struct RestoreLanguage(Language);
impl Drop for RestoreLanguage {
    fn drop(&mut self) {
        set_language(self.0);
    }
}

#[test]
fn system_detection_and_explicit_override() {
    for locale in ["en", "en_US.UTF-8", "fr-FR", "", "C"] {
        assert_eq!(Language::Auto.resolve(locale), Language::English);
    }
    for locale in ["zh", "zh-Hans-CN", "zh_CN.UTF-8", "ZH-SG"] {
        assert_eq!(Language::Auto.resolve(locale), Language::SimplifiedChinese);
    }
    assert_eq!(Language::English.resolve("zh-CN"), Language::English);
    assert_eq!(
        Language::SimplifiedChinese.resolve("en"),
        Language::SimplifiedChinese
    );
    assert_eq!(Language::from_name("invalid"), None);
    assert_eq!(
        primary_apple_language("(\n    \"zh-Hans-CN\",\n    en\n)\n"),
        Some("zh-Hans-CN")
    );
    assert_eq!(primary_apple_language("(en)"), Some("en"));
    assert_eq!(primary_apple_language("()"), None);
    assert_eq!(primary_apple_language("invalid"), None);
}

#[test]
fn switching_changes_labels_and_preserves_format_arguments() {
    let _restore = RestoreLanguage(language());
    let name = "中文 project {name}";
    set_language(Language::English);
    assert_eq!(text("设置"), "Settings");
    assert_eq!(text("随心输入"), "Ask anything");
    assert_eq!(
        super::format!("已编辑 {name}" => "Edited {name}"),
        "Edited 中文 project {name}"
    );
    assert_eq!(text("not a catalog key 中文"), "not a catalog key 中文");
    set_language(Language::SimplifiedChinese);
    assert_eq!(text("设置"), "设置");
    assert_eq!(
        super::format!("已编辑 {name}" => "Edited {name}"),
        "已编辑 中文 project {name}"
    );
}
