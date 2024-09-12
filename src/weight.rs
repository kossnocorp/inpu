use colored::Colorize;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::vec;
use swc_common::sync::Lrc;
use swc_common::{
    errors::{ColorConfig, Handler},
    SourceMap,
};
use swc_common::{Globals, Mark, GLOBALS};
use swc_ecma_codegen::text_writer::WriteJs;
use swc_ecma_codegen::Emitter;
use swc_ecma_minifier::optimize;
use swc_ecma_minifier::option::{CompressOptions, ExtraOptions, MangleOptions, MinifyOptions};
use swc_ecma_parser::{lexer::Lexer, Parser, StringInput, Syntax};
use swc_ecma_transforms_typescript::strip_type;
use swc_ecma_visit::{Visit, VisitMutWith, VisitWith};

#[derive(Debug)]
pub struct Weight {
    pub path: PathBuf,
    pub source: String,
    pub imports: Vec<String>,
    pub size: usize,
}

pub fn weight_command(path: &PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    let path = path.canonicalize().expect("The file doesn't exist");

    let mut queue = vec![path];
    let mut weights = vec![];

    while let Some(path) = queue.pop() {
        let weight = weight_file(path).expect("Failed to weight a file");
        let file_parent = weight.path.parent().unwrap_or(Path::new(""));

        let import_paths = weight
            .imports
            .iter()
            .map(|import| {
                file_parent
                    .join(import)
                    .canonicalize()
                    .expect("Can't find the file")
            })
            .collect::<Vec<PathBuf>>();

        weights.push(weight);

        import_paths.iter().for_each(|import_path| {
            if let None = weights.iter().find(|w| w.path == *import_path) {
                queue.push(import_path.clone());
            }
        })
    }

    let mut bundled_source: Vec<u8> = vec![];

    weights.into_iter().for_each(|weight| {
        println!("{}", weight.path.to_str().expect("Can't convert the path"));
        println!("");
        println!("Source code:");
        println!("");
        println!("{}", weight.source.dimmed());
        println!("");
        println!("Size:   {} bytes", weight.size);
        println!("Length: {}", weight.source.len());
        println!("");

        bundled_source.extend(weight.source.as_bytes());
    });

    let total_size = measure_compressed(&bundled_source);

    println!("Total size:   {} bytes", total_size);
    println!("Total length: {}", bundled_source.len());

    Ok(())
}

pub fn weight_file(path: PathBuf) -> Result<Weight, Box<dyn std::error::Error>> {
    let cm: Lrc<SourceMap> = Default::default();

    let handler = Handler::with_tty_emitter(ColorConfig::Auto, true, false, Some(cm.clone()));

    let source = cm
        .load_file(&path)
        .expect(format!("failed to load {:?}", path).as_str());

    let lexer = Lexer::new(
        Syntax::Typescript(Default::default()),
        Default::default(),
        StringInput::from(&*source),
        None,
    );

    let mut parser = Parser::new_from(lexer);

    for e in parser.take_errors() {
        e.into_diagnostic(&handler).emit();
    }

    let mut module = parser
        .parse_module()
        .map_err(|e| e.into_diagnostic(&handler).emit())
        .expect("failed to parser module");

    let mut imports = vec![];

    let mut visitor = Visitor {
        imports: &mut imports,
    };

    module.visit_with(&mut visitor);

    let mut ts_visitor = strip_type();
    module.visit_mut_with(&mut ts_visitor);

    let globals = Globals::new();

    let minified_program = GLOBALS.set(&globals, || {
        optimize(
            module.into(),
            cm.clone(),
            None,
            None,
            &MinifyOptions {
                compress: Some(CompressOptions {
                    ..Default::default()
                }),
                mangle: Some(MangleOptions {
                    ..Default::default()
                }),
                ..Default::default()
            },
            &ExtraOptions {
                unresolved_mark: Mark::new(),
                top_level_mark: Mark::new(),
                mangle_name_cache: None,
            },
        )
    });

    let minified_module = minified_program.as_module().unwrap();

    let mut code_buf = vec![];
    {
        let wr = Box::new(swc_ecma_codegen::text_writer::JsWriter::new(
            cm.clone(),
            "\n",
            &mut code_buf,
            None,
        )) as Box<dyn WriteJs>;

        let mut emitter = Emitter {
            cfg: swc_ecma_codegen::Config::default().with_minify(true),
            comments: None,
            cm: cm.clone(),
            wr,
        };

        emitter.emit_module(&minified_module).unwrap();
    }

    let size = measure_compressed(&code_buf);

    let output = String::from_utf8(code_buf).unwrap();

    let weight = Weight {
        path,
        imports,
        source: output,
        size,
    };

    Ok(weight)
}

fn measure_compressed(buf: &Vec<u8>) -> usize {
    let mut compressed_buf = vec![];
    {
        let mut writer = brotli::CompressorWriter::new(&mut compressed_buf, 4096, 11, 22);
        writer.write_all(buf).expect("Failed to compress");
    }
    compressed_buf.len()
}

struct Visitor<'a> {
    imports: &'a mut Vec<String>,
}

impl Visit for Visitor<'_> {
    fn visit_import_decl(&mut self, node: &swc_ecma_ast::ImportDecl) {
        self.imports.push(node.src.value.to_string());
    }
}
