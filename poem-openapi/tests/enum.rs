use poem_openapi::{
    Enum,
    registry::{MetaExternalDocument, MetaSchemaRef, Registry},
    types::{ParseFromJSON, ParseFromParameter, ToJSON, Type},
};
use serde_json::{Value, json};

#[test]
fn meta_enum_items() {
    #[derive(Enum, Debug, Eq, PartialEq)]
    enum MyEnum {
        CreateUser,
        DeleteUser,
    }

    let mut registry = Registry::new();
    MyEnum::register(&mut registry);
    let meta = registry.schemas.remove("MyEnum").unwrap();
    assert_eq!(
        meta.enum_items,
        vec![json!("CreateUser"), json!("DeleteUser")]
    );
}

#[test]
fn rename() {
    #[derive(Enum, Debug, Eq, PartialEq)]
    #[oai(rename = "AAA")]
    enum MyEnum {
        CreateUser,
        DeleteUser,
    }

    let mut registry = Registry::new();
    MyEnum::register(&mut registry);
    let meta = registry.schemas.remove("AAA").unwrap();
    assert_eq!(meta.ty, "string");
    assert_eq!(
        MyEnum::schema_ref(),
        MetaSchemaRef::Reference("AAA".to_string())
    );
}

#[test]
fn rename_all() {
    #[derive(Enum, Debug, Eq, PartialEq)]
    #[oai(rename_all = "camelCase")]
    enum MyEnum {
        CreateUser,
        DeleteUser,
    }

    assert_eq!(
        MyEnum::parse_from_json(Some(Value::String("createUser".to_string()))).unwrap(),
        MyEnum::CreateUser
    );

    assert_eq!(
        MyEnum::parse_from_json(Some(Value::String("deleteUser".to_string()))).unwrap(),
        MyEnum::DeleteUser
    );

    assert_eq!(
        MyEnum::CreateUser.to_json(),
        Some(Value::String("createUser".to_string()))
    );
    assert_eq!(
        MyEnum::DeleteUser.to_json(),
        Some(Value::String("deleteUser".to_string()))
    );
}

#[test]
fn rename_item() {
    #[derive(Enum, Debug, Eq, PartialEq)]
    enum MyEnum {
        CreateUser,
        #[oai(rename = "delete_user")]
        DeleteUser,
    }

    assert_eq!(
        MyEnum::parse_from_json(Some(Value::String("CreateUser".to_string()))).unwrap(),
        MyEnum::CreateUser
    );

    assert_eq!(
        MyEnum::parse_from_json(Some(Value::String("delete_user".to_string()))).unwrap(),
        MyEnum::DeleteUser
    );

    assert_eq!(
        MyEnum::CreateUser.to_json(),
        Some(Value::String("CreateUser".to_string()))
    );
    assert_eq!(
        MyEnum::DeleteUser.to_json(),
        Some(Value::String("delete_user".to_string()))
    );
}

#[test]
#[should_panic]
fn duplicate_name() {
    #[derive(Enum)]
    enum EnumA {
        A,
    }

    mod t {
        use super::*;

        #[derive(Enum)]
        pub enum EnumA {
            B,
        }
    }

    let mut registry = Registry::new();
    EnumA::register(&mut registry);
    t::EnumA::register(&mut registry);
}

#[test]
fn remote() {
    #[derive(Debug, Eq, PartialEq)]
    enum EnumA {
        A,
        B,
        C,
    }

    #[derive(Debug, Enum, Eq, PartialEq)]
    #[oai(remote = "EnumA")]
    enum EnumB {
        A,
        B,
        C,
    }

    let mut registry = Registry::new();
    EnumB::register(&mut registry);
    let meta = registry.schemas.remove("EnumB").unwrap();
    assert_eq!(meta.enum_items, vec![json!("A"), json!("B"), json!("C")]);

    let b: EnumB = EnumA::A.into();
    assert_eq!(b, EnumB::A);
    let b: EnumB = EnumA::B.into();
    assert_eq!(b, EnumB::B);
    let b: EnumB = EnumA::C.into();
    assert_eq!(b, EnumB::C);

    let a: EnumA = EnumB::A.into();
    assert_eq!(a, EnumA::A);
    let a: EnumA = EnumB::B.into();
    assert_eq!(a, EnumA::B);
    let a: EnumA = EnumB::C.into();
    assert_eq!(a, EnumA::C);
}

#[test]
fn description() {
    /// A
    ///
    /// AB
    /// CDE
    #[derive(Enum)]
    enum MyEnum {
        A,
    }

    let mut registry = Registry::new();
    MyEnum::register(&mut registry);
    let meta = registry.schemas.remove("MyEnum").unwrap();
    assert_eq!(meta.description, Some("A\n\nAB\nCDE"));
}

#[test]
fn deprecated() {
    #[derive(Enum)]
    enum MyEnumA {
        A,
    }

    let mut registry = Registry::new();
    MyEnumA::register(&mut registry);
    let meta = registry.schemas.remove("MyEnumA").unwrap();
    assert!(!meta.deprecated);

    #[derive(Enum)]
    #[oai(deprecated)]
    enum MyEnumB {
        A,
    }

    let mut registry = Registry::new();
    MyEnumB::register(&mut registry);
    let meta = registry.schemas.remove("MyEnumB").unwrap();
    assert!(meta.deprecated);
}

#[tokio::test]
async fn external_docs() {
    #[derive(Enum)]
    #[oai(
        external_docs = "https://github.com/OAI/OpenAPI-Specification/blob/main/versions/3.1.0.md"
    )]
    enum MyEnumB {
        A,
    }

    let mut registry = Registry::new();
    MyEnumB::register(&mut registry);
    let meta = registry.schemas.remove("MyEnumB").unwrap();
    assert_eq!(
        meta.external_docs,
        Some(MetaExternalDocument {
            url: "https://github.com/OAI/OpenAPI-Specification/blob/main/versions/3.1.0.md"
                .to_string(),
            description: None
        })
    );
}

#[test]
fn integer_enum_i32_schema_and_roundtrip() {
    #[derive(Enum, Debug, Eq, PartialEq, Clone, Copy)]
    #[repr(i32)]
    enum Int32Enum {
        Zero = 0,
        One = 1,
        Two = 2,
    }

    // Schema checks
    let mut registry = Registry::new();
    Int32Enum::register(&mut registry);
    let meta = registry.schemas.remove("Int32Enum").unwrap();

    assert_eq!(meta.ty, "integer");
    assert_eq!(meta.format, Some("int32"));
    assert_eq!(meta.enum_items, vec![json!(0), json!(1), json!(2)]);
    assert_eq!(meta.minimum, None);
    assert_eq!(meta.maximum, None);

    // JSON roundtrip
    assert_eq!(
        Int32Enum::parse_from_json(Some(json!(1))).unwrap(),
        Int32Enum::One
    );
    assert_eq!(Int32Enum::Two.to_json(), Some(json!(2)));

    // Parameter roundtrip
    assert_eq!(
        Int32Enum::parse_from_parameter("0").unwrap(),
        Int32Enum::Zero
    );
}

#[test]
fn integer_enum_u32_schema_bounds_and_roundtrip() {
    #[derive(Enum, Debug, Eq, PartialEq, Clone, Copy)]
    #[oai(repr = "u32")]
    enum U32Enum {
        Zero = 0,
        One = 1,
        Two = 2,
    }

    // Schema checks
    let mut registry = Registry::new();
    U32Enum::register(&mut registry);
    let meta = registry.schemas.remove("U32Enum").unwrap();

    assert_eq!(meta.ty, "integer");
    // In our derive we emit int64 for unsigned (OAS 3.0 has no uint32):
    assert_eq!(meta.format, Some("int64"));
    assert_eq!(meta.enum_items, vec![json!(0), json!(1), json!(2)]);
    // Unsigned bounds present
    assert_eq!(meta.minimum, Some(0.0));
    assert_eq!(meta.maximum, Some(4294967295.0));

    // JSON roundtrip
    assert_eq!(
        U32Enum::parse_from_json(Some(json!(2))).unwrap(),
        U32Enum::Two
    );
    assert_eq!(U32Enum::One.to_json(), Some(json!(1)));

    // Parameter roundtrip
    assert_eq!(U32Enum::parse_from_parameter("1").unwrap(), U32Enum::One);
}
