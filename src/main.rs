use swc_common::{input::StringInput, sync::Lrc, FileName, SourceMap};
use swc_ecma_parser::{lexer::Lexer, Parser, Syntax, EsConfig};
use swc_ecma_visit::Visit;
use swc_ecma_ast::{Module, ImportDecl, ImportSpecifier, Ident};
use std::collections::{HashMap, HashSet};

// Struct to hold import information
struct ImportCollector {
    imports: HashMap<String, ImportSpecifierType>,
}

#[derive(Debug, PartialEq, Eq)]
enum ImportSpecifierType {
    Named(String),    // import { foo } from 'module'
    Default(String),  // import foo from 'module'
    Namespace(String),// import * as foo from 'module'
}

impl ImportCollector {
    fn new() -> Self {
        Self {
            imports: HashMap::new(),
        }
    }
}

impl Visit for ImportCollector {
    fn visit_import_decl(&mut self, import_decl: &ImportDecl) {
        let source = import_decl.src.value.to_string();
        println!("Found import from: {}", source);
        for specifier in &import_decl.specifiers {
            match specifier {
                ImportSpecifier::Named(named) => {
                    let imported = match &named.imported {
                        Some(i) => match i {
                            swc_ecma_ast::ModuleExportName::Ident(id) => id.sym.to_string(),
                            swc_ecma_ast::ModuleExportName::Str(s) => s.value.to_string(),
                        },
                        None => named.local.sym.to_string(),
                    };
                    let local = named.local.sym.to_string();
                    self.imports.insert(local.clone(), ImportSpecifierType::Named(imported));
                },
                ImportSpecifier::Default(default) => {
                    let local = default.local.sym.to_string();
                    self.imports.insert(local.clone(), ImportSpecifierType::Default(local));
                },
                ImportSpecifier::Namespace(ns) => {
                    let local = ns.local.sym.to_string();
                    self.imports.insert(local.clone(), ImportSpecifierType::Namespace(local));
                },
            }
        }
    }
}

// Struct to collect all used identifiers
struct IdentifierCollector {
    used_identifiers: HashSet<String>,
}

impl IdentifierCollector {
    fn new() -> Self {
        Self {
            used_identifiers: HashSet::new(),
        }
    }
}

impl Visit for IdentifierCollector {
    fn visit_ident(&mut self, ident: &Ident) {
        println!("Found identifier: {} (raw)", ident.sym);
        self.used_identifiers.insert(ident.sym.to_string());
    }

    fn visit_new_expr(&mut self, new_expr: &swc_ecma_ast::NewExpr) {
        match &*new_expr.callee {
            swc_ecma_ast::Expr::Ident(ident) => {
                println!("Found identifier in new: {}", ident.sym);
                self.used_identifiers.insert(ident.sym.to_string());
            }
            _ => {}
        }
    }

    fn visit_import_decl(&mut self, import: &ImportDecl) {
        // Skip recording regular import identifiers unless they're type imports
        if !import.type_only {
            return;
        }
    }

    fn visit_ts_type_ref(&mut self, type_ref: &swc_ecma_ast::TsTypeRef) {
        match &type_ref.type_name {
            swc_ecma_ast::TsEntityName::Ident(ident) => {
                self.used_identifiers.insert(ident.sym.to_string());
            },
            swc_ecma_ast::TsEntityName::TsQualifiedName(qual) => {
                // Handle cases like Types.SomeInterface
                match &qual.left {
                    swc_ecma_ast::TsEntityName::Ident(left) => {
                        self.used_identifiers.insert(left.sym.to_string());
                    },
                    _ => {}
                }
                self.used_identifiers.insert(qual.right.sym.to_string());
            }
        }
    }

    fn visit_ts_type_ann(&mut self, type_ann: &swc_ecma_ast::TsTypeAnn) {
        match &*type_ann.type_ann {
            swc_ecma_ast::TsType::TsTypeRef(ref type_ref) => {
                self.visit_ts_type_ref(type_ref);
            }
            _ => {}
        }
    }

    fn visit_ts_interface_decl(&mut self, interface: &swc_ecma_ast::TsInterfaceDecl) {
        // Visit extends clause
        for extend in &interface.extends {
            self.visit_ts_expr_with_type_args(extend);
        }
        // Visit each member of the interface body
        for member in &interface.body.body {
            match member {
                swc_ecma_ast::TsTypeElement::TsPropertySignature(prop) => {
                    if let Some(type_ann) = &prop.type_ann {
                        self.visit_ts_type_ann(type_ann);
                    }
                }
                _ => {}
            }
        }
    }

    fn visit_ts_expr_with_type_args(&mut self, type_args: &swc_ecma_ast::TsExprWithTypeArgs) {
        if let swc_ecma_ast::Expr::Ident(ref ident) = *type_args.expr {
            self.used_identifiers.insert(ident.sym.to_string());
        }
    }

    fn visit_tagged_tpl(&mut self, tpl: &swc_ecma_ast::TaggedTpl) {
        // Handle styled-components and emotion template literals
        if let swc_ecma_ast::Expr::Member(member) = &*tpl.tag {
            if let swc_ecma_ast::Expr::Ident(obj) = &*member.obj {
                // Record the base identifier (e.g., 'styled' in styled.div)
                self.used_identifiers.insert(obj.sym.to_string());
            }
        } else if let swc_ecma_ast::Expr::Ident(ident) = &*tpl.tag {
            // Record direct identifier usage (e.g., css`...`)
            self.used_identifiers.insert(ident.sym.to_string());
        }
    }
}

fn parse_js(code: &str) -> Module {
    let cm: Lrc<SourceMap> = Default::default();
    let fm = cm.new_source_file(FileName::Custom("file.js".into()), code.into());

    let lexer = Lexer::new(
        Syntax::Typescript(swc_ecma_parser::TsConfig {
            tsx: true,
            decorators: true,
            ..Default::default()
        }),
        Default::default(),
        StringInput::from(&*fm),
        None,
    );

    let mut parser = Parser::new_from(lexer);
    parser.parse_module().expect("Failed to parse module")
}

fn find_unused_imports(module: &Module) -> Vec<String> {
    // Collect imports
    let mut import_collector = ImportCollector::new();
    import_collector.visit_module(module);
    
    // Collect used identifiers
    let mut identifier_collector = IdentifierCollector::new();
    identifier_collector.visit_module(module);
    
    // Determine unused imports


    let mut unused = Vec::new();
    for (imported, _) in &import_collector.imports {
        if !identifier_collector.used_identifiers.contains(imported) {
            unused.push(imported.clone());
        }
    }
    
    unused
}

fn main() {
    let code = r#"
        import { MyClass } from './my-class';
        import { AnotherClass } from './another';
        import { format, parse } from 'date-fns';
        import { styled, css } from '@emotion/styled';
        import * as StyleUtils from './style-utils';

        function test() {
            // Only using MyClass in new expression
            return new MyClass();
        }

        // Using namespace import in type position
        type CustomTheme = StyleUtils.Theme;
        
        interface Props extends BaseProps {
            user: User;
            theme: Theme;
            colors?: Colors;
        }

        // Type with generic constraints
        type UserWithPrefs<T extends UserPreferences = UserPreferences> = {
            user: User;
            preferences: T;
        };

        // Union type usage
        type ThemeMode = 'light' | 'dark';

        // Utility type usage
        type PartialTheme = Partial<Theme>;

        // Type guard function
        function isUserWithPrefs(obj: any): obj is UserWithPrefs {
            return obj && obj.user && obj.preferences;
        }

        // Styled component with TypeScript
        // CSS-in-JS with styled-components/emotion
        const StyledDiv = styled.div<{ theme: Theme }>`
            color: ${props => props.theme.color};
            ${props => css`
                background: ${props.theme.background};
            `}
        `;

        // Using namespace import for styles
        const ExtraStyles = StyleUtils.css`
            padding: 1rem;
        `;

        // React component with multiple type usages
        const MyComponent: FC<Props> = ({ user, theme, colors }) => {
            // Generic useState with complex type
            const [data, setData] = useState<UserWithPrefs | null>(null);
            
            // Type usage in function parameters
            const handleThemeChange = (newTheme: PartialTheme) => {
                if (!isEmpty(newTheme)) {
                    // Do something
                }
            };

            const formatDate = (date: Date): string => {
                return format(date, 'yyyy-MM-dd');
            };

            return (
                <StyledDiv theme={theme}>
                    Hello {user.name}
                </StyledDiv>
            );
        };

        export default MyComponent;
    "#;

    let module = parse_js(code);
    let unused = find_unused_imports(&module);

    if unused.is_empty() {
        println!("No unused imports found.");
    } else {
        println!("Unused imports:");
        for imp in unused {
            println!("- {}", imp);
        }
    }
}
