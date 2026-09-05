use xtask::legacy_migration::extract_swift_test_methods;

#[test]
fn swift_inventory_extracts_only_public_test_declarations()
{
   let source = r#"
final class Sample: XCTestCase
{
   func testOne()
   {
   }

   private func helper()
   {
   }

   func testTwo_withSuffix() throws
   {
   }
}
"#;
   assert_eq!(extract_swift_test_methods(source).expect("extract methods"), vec![String::from("testOne"), String::from("testTwo_withSuffix")]);
}

#[test]
fn malformed_test_declaration_fails_closed()
{
   assert!(extract_swift_test_methods("func testMissingArgumentList").is_err());
   assert!(extract_swift_test_methods("func testInvalid-name() {}").is_err());
}
