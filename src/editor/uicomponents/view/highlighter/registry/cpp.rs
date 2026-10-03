use super::LanguageConfig;

pub const CPP: LanguageConfig = LanguageConfig {

    name: "C++",

    extensions: &["cpp","cc","cxx","hpp","hh","hxx"],

    keywords: &[
        "alignas","alignof","and","and_eq","asm","auto","bitand","bitor","break","case","catch",
        "char","char8_t","char16_t","char32_t","class","compl","concept","const","consteval",
        "constexpr","constinit","const_cast","continue","co_await","co_return","co_yield",
        "decltype","default","delete","do","double","dynamic_cast","else","enum","explicit",
        "export","extern","false","float","for","friend","goto","if","inline","int","long",
        "mutable","namespace","new","noexcept","not","not_eq","nullptr","operator","or","or_eq",
        "private","protected","public","reflexpr","register","reinterpret_cast","requires",
        "return","short","signed","sizeof","static","static_assert","static_cast","struct",
        "switch","template","this","thread_local","throw","true","try","typedef","typeid",
        "typename","union","unsigned","using","virtual","void","volatile","wchar_t","while",
        "xor","xor_eq",
    ],

    types: &[
        "bool","char","char8_t","char16_t","char32_t","double","float","int","long","short",
        "signed","unsigned","void","wchar_t","size_t","string",
    ],

    known_values: &["true","false","nullptr","NULL"],

    line_comment: Some("//"),

    block_comment: Some(("/*", "*/")),

    supports_char_literal: true,

    supports_lifetime: false,

    string_delimiters: &[
        ("\"", "\""),
        ("R\"", "\""),
    ],

};