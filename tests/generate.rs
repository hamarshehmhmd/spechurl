use std::fs;
use std::path::Path;

use spechurl::{check_suite, generate_from_str, generate_suite, write_suite};

fn spec(name: &str) -> spechurl::Suite {
    generate_suite(Path::new(name)).unwrap_or_else(|err| panic!("generate {name}: {err}"))
}

#[test]
fn readme_quickstart_matches_generated_post_pets() {
    let suite = spec("fixtures/petstore.yaml");
    let readme = fs::read_to_string("README.md").unwrap();
    let marker = "```hurl\n";
    let start = readme
        .find(marker)
        .expect("README should show a hurl example");
    let rest = &readme[start + marker.len()..];
    let end = rest.find("\n```").expect("hurl fence should close");
    assert_eq!(
        suite.files["post_pets.hurl"].trim_end(),
        rest[..end].trim_end(),
        "README quickstart example drifted from spechurl generate"
    );
}

#[test]
fn petstore_emits_one_file_per_operation() {
    let suite = spec("fixtures/petstore.yaml");
    let names: Vec<_> = suite
        .files
        .keys()
        .filter(|name| name.ends_with(".hurl"))
        .cloned()
        .collect();
    assert_eq!(
        names,
        vec![
            "delete_pets_petid.hurl",
            "get_pets.hurl",
            "get_pets_petid.hurl",
            "post_pets.hurl",
            "put_pets_petid.hurl",
        ]
    );
    assert!(suite.files.contains_key("README.md"));

    let list = &suite.files["get_pets.hurl"];
    assert!(list.contains("GET {{base_url}}/pets\n"));
    assert!(list.contains("[Query]\nlimit: 20\nstatus: available\n"));
    assert!(!list.contains("\nq:"));
    assert!(list.contains("HTTP 200\n"));
    assert!(list.contains("header \"Content-Type\" contains \"application/json\""));
    assert!(list.contains("operationId: listPets"));

    let create = &suite.files["post_pets.hurl"];
    assert!(create.contains("POST {{base_url}}/pets\n"));
    assert!(create.contains("HTTP 201\n"));
    assert!(create.contains("\"name\": \"Fluffy\""));
    assert!(create.contains("\"photoUrls\""));
    let name_at = create.find("\"name\"").unwrap();
    let tag_at = create.find("\"tag\"").unwrap();
    assert!(name_at < tag_at, "object keys are sorted");

    let show = &suite.files["get_pets_petid.hurl"];
    assert!(show.contains("GET {{base_url}}/pets/123\n"));

    let delete = &suite.files["delete_pets_petid.hurl"];
    assert!(delete.contains("DELETE {{base_url}}/pets/123\n"));
    assert!(delete.contains("HTTP 204\n"));
    assert!(!delete.contains("[Asserts]"));

    let update = &suite.files["put_pets_petid.hurl"];
    assert!(update.contains("\"name\": \"Fluffy\""));
    assert!(
        !update.contains("\"id\""),
        "readOnly properties stay out of request bodies"
    );

    let readme = &suite.files["README.md"];
    assert!(readme.contains("petstore.yaml"));
    assert!(readme.contains("https://petstore.example.com"));
    assert!(readme.contains("hurl --variable base_url=https://petstore.example.com --test ."));
}

#[test]
fn edge_cases_cover_path_headers_and_json_body() {
    let suite = spec("fixtures/edge-cases.yaml");
    let get = &suite.files["get_projects_projectid_items_itemid.hurl"];
    assert!(get.contains("GET {{base_url}}/projects/proj%20acme/items/42\n"));
    assert!(get.contains("X-Request-Id: 00000000-0000-0000-0000-000000000001\n"));
    assert!(get.contains("Accept: application/json\n"));
    assert!(get.contains("[Query]\ninclude: owner\n"));
    assert!(
        !get.contains("limit:"),
        "optional query without a sample is omitted"
    );
    assert!(get.contains("cookie parameters are not emitted"));
    assert!(get.contains("HTTP 200\n"));

    let create = &suite.files["post_projects_projectid_items.hurl"];
    assert!(create.contains("POST {{base_url}}/projects/proj%20acme/items\n"));
    assert!(create.contains("Idempotency-Key: idem-1\n"));
    assert!(create.contains("\"name\": \"Widget\""));
    assert!(create.contains("\"count\": 2"));
    assert!(create.contains("\"active\": true"));
    assert!(create.contains("HTTP 201\n"));
    assert!(create.contains("other documented success statuses: 202"));
    assert!(create.contains("request body uses application/json; also documented: text/plain"));
    assert!(create.contains("https://edge.example.com/v1"));
    assert!(!create.contains("https://edge.example.com/v1/"));

    let search = &suite.files["get_search.hurl"];
    assert!(search.contains("no 2xx response is documented; asserting 404"));
    assert!(search.contains("HTTP 404\n"));
}

#[test]
fn openapi_31_json_uses_const_examples_and_json_content_type() {
    let suite = spec("fixtures/widgets.openapi.json");
    let create = &suite.files["post_widgets.hurl"];
    assert!(create.contains("\"count\": 1"));
    assert!(create.contains("\"name\": \"Cog\""));
    assert!(!create.contains("\"label\""));
    assert!(!create.contains("\"active\""));
    assert!(create.contains("Content-Type: application/json\n"));
    assert!(create.contains("HTTP 200\n"));
    assert!(create.contains("other documented success statuses: 2XX"));
    assert!(create.contains("header \"Content-Type\" contains \"application/vnd.api+json\""));
    assert!(suite.files["README.md"].contains("3.1.0"));
}

#[test]
fn output_is_deterministic_and_newline_terminated() {
    let first = spec("fixtures/petstore.yaml");
    let second = spec("fixtures/petstore.yaml");
    assert_eq!(first, second);
    for contents in first.files.values() {
        assert!(contents.ends_with('\n'));
        assert!(!contents.ends_with("\n\n"));
        assert!(!contents.contains('\r'));
    }
}

#[test]
fn check_accepts_a_fresh_suite_and_rejects_drift() {
    let dir = tempfile::tempdir().unwrap();
    let suite = spec("fixtures/petstore.yaml");
    write_suite(&suite, dir.path()).unwrap();
    let count = check_suite(Path::new("fixtures/petstore.yaml"), dir.path()).unwrap();
    assert_eq!(count, suite.files.len());

    let target = dir.path().join("get_pets.hurl");
    let mut text = fs::read_to_string(&target).unwrap();
    text = text.replace("HTTP 200", "HTTP 599");
    fs::write(&target, text).unwrap();
    let err = check_suite(Path::new("fixtures/petstore.yaml"), dir.path()).unwrap_err();
    let message = err.to_string();
    assert!(message.contains("mismatch: get_pets.hurl"));
    assert_eq!(err.exit_code(), 1);

    fs::write(
        dir.path().join("extra.hurl"),
        "GET {{base_url}}/extra\n\nHTTP 200\n",
    )
    .unwrap();
    let message = check_suite(Path::new("fixtures/petstore.yaml"), dir.path())
        .unwrap_err()
        .to_string();
    assert!(message.contains("unexpected: extra.hurl"));

    fs::remove_file(dir.path().join("post_pets.hurl")).unwrap();
    let message = check_suite(Path::new("fixtures/petstore.yaml"), dir.path())
        .unwrap_err()
        .to_string();
    assert!(message.contains("missing: post_pets.hurl"));
}

#[test]
fn invalid_documents_have_clear_errors() {
    let missing = generate_suite(Path::new("fixtures/nope.yaml")).unwrap_err();
    assert!(missing.to_string().contains("could not read"));
    assert_eq!(missing.exit_code(), 2);

    let broken = generate_from_str("broken.yaml", "openapi: [\n").unwrap_err();
    assert!(broken.to_string().contains("could not parse"));

    let swagger = generate_from_str(
        "swagger.yaml",
        "swagger: \"2.0\"\ninfo:\n  title: Old\n  version: \"1\"\npaths: {}\n",
    )
    .unwrap_err();
    assert!(swagger.to_string().contains("OpenAPI 2.0"));

    let version = generate_from_str(
        "v4.yaml",
        "openapi: 4.0.0\ninfo:\n  title: Future\n  version: \"1\"\npaths:\n  /x:\n    get:\n      responses:\n        \"200\":\n          description: ok\n",
    )
    .unwrap_err();
    assert!(version.to_string().contains("unsupported OpenAPI version"));

    let external = generate_from_str(
        "external.yaml",
        "openapi: 3.0.3\ninfo:\n  title: E\n  version: \"1\"\npaths:\n  /x:\n    get:\n      parameters:\n        - $ref: \"./other.yaml#/components/parameters/Id\"\n      responses:\n        \"200\":\n          description: ok\n",
    )
    .unwrap_err();
    assert!(external.to_string().contains("external $ref"));

    let unresolved = generate_from_str(
        "missing-ref.yaml",
        "openapi: 3.0.3\ninfo:\n  title: E\n  version: \"1\"\npaths:\n  /x/{id}:\n    get:\n      parameters:\n        - $ref: \"#/components/parameters/Missing\"\n      responses:\n        \"200\":\n          description: ok\n",
    )
    .unwrap_err();
    assert!(unresolved.to_string().contains("unresolved $ref"));

    let empty = generate_from_str(
        "empty.yaml",
        "openapi: 3.1.0\ninfo:\n  title: E\n  version: \"1\"\npaths: {}\n",
    )
    .unwrap_err();
    assert!(empty.to_string().contains("no paths"));
}

#[test]
fn schema_samples_cover_cycles_allof_and_minimums() {
    let source = r##"
openapi: 3.0.3
info:
  title: Samples
  version: "1"
paths:
  /nodes:
    post:
      operationId: createNode
      requestBody:
        required: true
        content:
          application/json:
            schema:
              $ref: "#/components/schemas/Node"
      responses:
        "204":
          description: stored
  /limits:
    get:
      parameters:
        - name: page
          in: query
          required: true
          schema:
            type: integer
            minimum: 1
      responses:
        "2XX":
          description: range
components:
  schemas:
    Node:
      type: object
      required: [name, child, extra]
      properties:
        name:
          type: string
        child:
          $ref: "#/components/schemas/Node"
        extra:
          allOf:
            - type: object
              required: [left]
              properties:
                left:
                  type: string
                  example: L
            - type: object
              required: [right]
              properties:
                right:
                  type: integer
                  example: 2
"##;
    let suite = generate_from_str("samples.yaml", source).unwrap();
    let body = &suite.files["post_nodes.hurl"];
    assert!(body.contains("\"name\": \"string\""));
    assert!(body.contains("\"child\": null"));
    assert!(body.contains("\"left\": \"L\""));
    assert!(body.contains("\"right\": 2"));
    let limits = &suite.files["get_limits.hurl"];
    assert!(limits.contains("[Query]\npage: 1\n"));
    assert!(limits.contains("HTTP *\n"));
    assert!(limits.contains("status >= 200\n"));
    assert!(limits.contains("status < 300\n"));
}

#[test]
fn check_treats_crlf_as_the_same_suite() {
    let dir = tempfile::tempdir().unwrap();
    let suite = spec("fixtures/petstore.yaml");
    write_suite(&suite, dir.path()).unwrap();
    let path = dir.path().join("README.md");
    let text = fs::read_to_string(&path).unwrap().replace('\n', "\r\n");
    fs::write(&path, text).unwrap();
    check_suite(Path::new("fixtures/petstore.yaml"), dir.path()).unwrap();
}

#[test]
fn form_and_text_bodies() {
    let source = r#"
openapi: 3.0.3
info:
  title: Bodies
  version: "1"
paths:
  /login:
    post:
      requestBody:
        required: true
        content:
          application/x-www-form-urlencoded:
            schema:
              type: object
              required: [username, password]
              properties:
                username:
                  type: string
                  example: ada
                password:
                  type: string
                  example: "secret#1"
      responses:
        "204":
          description: signed in
  /note:
    put:
      requestBody:
        required: true
        content:
          text/plain:
            schema:
              type: string
              example: hello
      responses:
        "200":
          description: stored
          content:
            text/plain:
              schema:
                type: string
"#;
    let suite = generate_from_str("bodies.yaml", source).unwrap();
    let login = &suite.files["post_login.hurl"];
    assert!(login.contains("[Form]\npassword: secret\\#1\nusername: ada\n"));
    assert!(!login.contains("Content-Type:"));
    let note = &suite.files["put_note.hurl"];
    assert!(note.contains("Content-Type: text/plain\n`hello`\n"));
    assert!(note.contains("header \"Content-Type\" contains \"text/plain\""));
}

#[test]
fn colliding_paths_get_stable_suffixes() {
    let source = r#"
openapi: 3.0.3
info:
  title: Collide
  version: "1"
paths:
  /a/b:
    get:
      responses:
        "200":
          description: first
  /a_b:
    get:
      responses:
        "200":
          description: second
"#;
    let suite = generate_from_str("collide.yaml", source).unwrap();
    assert!(suite.files.contains_key("get_a_b.hurl"));
    assert!(suite.files.contains_key("get_a_b_2.hurl"));
    assert!(suite.files["get_a_b.hurl"].contains("GET /a/b"));
    assert!(suite.files["get_a_b_2.hurl"].contains("GET /a_b"));
}
