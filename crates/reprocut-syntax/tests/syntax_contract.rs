use reprocut_syntax::{deletion_transforms, hoist_transforms, parse_valid, SyntaxLanguage};

#[test]
fn each_bundled_language_emits_a_reparse_valid_function_deletion() {
    let cases: [(SyntaxLanguage, &[u8], &str); 9] = [
        (
            SyntaxLanguage::Rust,
            b"fn keep() {}\nfn drop_me() {}\n",
            "function_item",
        ),
        (
            SyntaxLanguage::Python,
            b"def keep():\n    pass\n\ndef drop_me():\n    pass\n",
            "function_definition",
        ),
        (
            SyntaxLanguage::JavaScript,
            b"function keep() {}\nfunction dropMe() {}\n",
            "function_declaration",
        ),
        (
            SyntaxLanguage::TypeScript,
            b"function keep(): void {}\nfunction dropMe(): void {}\n",
            "function_declaration",
        ),
        (
            SyntaxLanguage::Tsx,
            b"function Keep() { return <div/>; }\nfunction Drop() { return <span/>; }\n",
            "function_declaration",
        ),
        (
            SyntaxLanguage::C,
            b"int keep(void) { return 1; }\nint drop_me(void) { return 2; }\n",
            "function_definition",
        ),
        (
            SyntaxLanguage::Cpp,
            b"int keep() { return 1; }\nint drop_me() { return 2; }\n",
            "function_definition",
        ),
        (
            SyntaxLanguage::Go,
            b"package main\nfunc keep() int { return 1 }\nfunc dropMe() int { return 2 }\n",
            "function_declaration",
        ),
        (
            SyntaxLanguage::Java,
            b"class Demo { int keep() { return 1; } int dropMe() { return 2; } }\n",
            "method_declaration",
        ),
    ];
    for (language, source, expected_kind) in cases {
        let transforms = deletion_transforms(language, source).expect("valid source");
        let transform = transforms
            .iter()
            .find(|transform| transform.kind() == expected_kind)
            .expect("function deletion");
        let candidate = transform.candidate_bytes(source).expect("candidate");
        parse_valid(language, &candidate).expect("candidate reparses");
    }
}

#[test]
fn native_language_extensions_select_the_expected_grammar() {
    let cases = [
        ("main.c", SyntaxLanguage::C),
        ("main.h", SyntaxLanguage::C),
        ("main.cc", SyntaxLanguage::Cpp),
        ("main.cpp", SyntaxLanguage::Cpp),
        ("main.cxx", SyntaxLanguage::Cpp),
        ("main.hh", SyntaxLanguage::Cpp),
        ("main.hpp", SyntaxLanguage::Cpp),
        ("main.hxx", SyntaxLanguage::Cpp),
        ("main.go", SyntaxLanguage::Go),
        ("Main.java", SyntaxLanguage::Java),
    ];
    for (path, expected) in cases {
        assert_eq!(
            SyntaxLanguage::from_path(std::path::Path::new(path)),
            Some(expected)
        );
    }
}

#[test]
fn invalid_or_non_utf8_source_is_rejected_before_transform_generation() {
    assert!(deletion_transforms(SyntaxLanguage::Rust, b"fn broken(").is_err());
    assert!(deletion_transforms(SyntaxLanguage::Python, &[0xff, 0xfe]).is_err());
}

#[test]
fn child_hoists_are_emitted_only_when_the_candidate_reparses() {
    let source = b"const value = ((1 + 2));\nconsole.log(value);\n";
    let transforms = hoist_transforms(SyntaxLanguage::JavaScript, source).expect("valid source");
    assert!(!transforms.is_empty());
    for transform in transforms {
        let candidate = transform.candidate_bytes(source).expect("candidate");
        parse_valid(SyntaxLanguage::JavaScript, &candidate).expect("valid hoist");
    }
}
