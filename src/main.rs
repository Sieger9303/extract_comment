// Cargo.toml
// [dependencies]
// csv = "1.1"
// serde = { version = "1.0", features = ["derive"] }
// serde_json = "1.0"
// syn = { version = "1.0", features = ["full"] }
// quote = "1.0"

use core::panic;
use std::env;
use std::fs;
use std::fs::OpenOptions;
use std::fs::ReadDir;
use std::path::{Path, PathBuf};
use std::io::Write;

use csv::ReaderBuilder;
use serde::Serialize;
use syn::token::Impl;
use syn::ForeignItem;
use syn::ForeignItemFn;
use syn::ForeignItemMacro;
use syn::ImplItemMacro;
use syn::ImplItemMethod;
use syn::ItemMacro;
use syn::ItemMacro2;
use syn::{File, Item, ItemFn, spanned::Spanned};

use walkdir::WalkDir;
use flate2::read::GzDecoder;
use tar::Archive;
use anyhow::{Context, Result};

/// 用于保存目标函数的注释状态及内容
#[derive(Debug, Serialize)]
struct FunctionCommentStatus {
    crate_name:String,
    def_path: String,
    file: String,
    line: usize,
    has_doc: bool,
    doc_paragraph: String,
    has_inline_comment: bool,
    inline_comment_paragraph: String,
}

/// 使用 syn 提取函数中的文档注释（通过 #[doc = "..."] 属性）
fn extract_doc_comments(func: &FunctionMacroType) -> Vec<String> {
    match func{
        FunctionMacroType::ItemFn(item_fn) => {
                            item_fn.attrs
                            .iter()
                            .filter_map(|attr| {
                                if attr.path.is_ident("doc") {
                                    if let Ok(syn::Meta::NameValue(meta)) = attr.parse_meta() {
                                        if let syn::Lit::Str(lit) = meta.lit {
                                            return Some(lit.value());
                                        }
                                    }
                                }
                                None
                            })
                            .collect()
                },
        FunctionMacroType::ForeignItemFn(foreign_item_fn) => {
                    foreign_item_fn.attrs
                    .iter()
                    .filter_map(|attr| {
                        if attr.path.is_ident("doc") {
                            if let Ok(syn::Meta::NameValue(meta)) = attr.parse_meta() {
                                if let syn::Lit::Str(lit) = meta.lit {
                                    return Some(lit.value());
                                }
                            }
                        }
                        None
                    })
                    .collect()
                },
        FunctionMacroType::ImplItemMethod(impl_item_method) => {
                    impl_item_method.attrs
                    .iter()
                    .filter_map(|attr| {
                        if attr.path.is_ident("doc") {
                            if let Ok(syn::Meta::NameValue(meta)) = attr.parse_meta() {
                                if let syn::Lit::Str(lit) = meta.lit {
                                    return Some(lit.value());
                                }
                            }
                        }
                        None
                    })
                    .collect()
                },
        FunctionMacroType::ItemMacro(item_macro) => {
            item_macro.attrs
            .iter()
            .filter_map(|attr| {
                if attr.path.is_ident("doc") {
                    if let Ok(syn::Meta::NameValue(meta)) = attr.parse_meta() {
                        if let syn::Lit::Str(lit) = meta.lit {
                            return Some(lit.value());
                        }
                    }
                }
                None
            })
            .collect()
        },
        FunctionMacroType::ItemMacro2(item_macro2) =>{
            item_macro2.attrs
            .iter()
            .filter_map(|attr| {
                if attr.path.is_ident("doc") {
                    if let Ok(syn::Meta::NameValue(meta)) = attr.parse_meta() {
                        if let syn::Lit::Str(lit) = meta.lit {
                            return Some(lit.value());
                        }
                    }
                }
                None
            })
            .collect()
        },
        /*FunctionMacroType::ForeignItemMacro(foreign_item_macro) => {
            foreign_item_macro.attrs
            .iter()
            .filter_map(|attr| {
                if attr.path.is_ident("doc") {
                    if let Ok(syn::Meta::NameValue(meta)) = attr.parse_meta() {
                        if let syn::Lit::Str(lit) = meta.lit {
                            return Some(lit.value());
                        }
                    }
                }
                None
            })
            .collect()
        },
        FunctionMacroType::ImplItemMacro(impl_item_macro) => {
            impl_item_macro.attrs
            .iter()
            .filter_map(|attr| {
                if attr.path.is_ident("doc") {
                    if let Ok(syn::Meta::NameValue(meta)) = attr.parse_meta() {
                        if let syn::Lit::Str(lit) = meta.lit {
                            return Some(lit.value());
                        }
                    }
                }
                None
            })
            .collect()
        },*/
    }
}

/// 从给定的多行文本（每一行为一个 &str）中提取所有注释（支持单行 // 注释和块注释 /* ... */，并正确处理嵌套）
fn extract_comments_from_lines(lines: &[&str]) -> Vec<String> {
    //let mut res_before_comment=Vec::new();
    let mut comments = Vec::new();
    let mut commentStack = Vec::new();         // 块注释嵌套计数器
    let mut current_block = String::new(); // 当前正在收集的块注释内容
    //let mut inside_doc=0;
    let mut i=0;
    for line in lines{
        //println!("before a line {:?}",comments);
        //let line=lines[i];
        let chars: Vec<char> = line.chars().collect();
        let mut pos = 0;
        //println!("now line: {}",line);
        while pos < chars.len() {
            //println!("now char: {}", chars[pos]);
            //println!("now current_block: {}", current_block);
            if commentStack.is_empty() {
                // 检查是否是单行注释
                if pos + 1 < chars.len() && chars[pos] == '/' && chars[pos + 1] == '/' {
                    // 直接将本行后半部分作为单行注释
                    if (pos + 2 < chars.len()&& chars[pos+2]!='/' && chars[pos+2]!='!')
                    {
                        //println!("before push {:?}",comments);
                        let comment: String = chars[pos..].iter().collect();
                        //println!("push // {}",comment);
                        comments.push(comment.trim().to_string());
                        //println!("after push // {:?}",comments);
                        break; // 当前行处理完毕
                    }
                    else{
                        break;
                    }
                }
                // 检查是否是块注释的起始标记 "/*"
                else if pos + 1 < chars.len() && chars[pos] == '/' && chars[pos + 1] == '*' {
                    if (pos + 2 < chars.len()&&chars[pos+1]!='*'&&chars[pos+2]!='!')
                    {
                        commentStack.push(commentType::inline);
                        current_block.push_str("/*");
                        pos += 2;
                    }else{
                        commentStack.push(commentType::doc);
                        pos += 3;
                    }
                } else {
                    pos += 1;
                }
            } else {
                // 已经在块注释中，处理嵌套情况
                if pos + 1 < chars.len() && chars[pos] == '/' && chars[pos + 1] == '*' {
                    commentStack.push(commentType::inline);
                    if let commentType::inline=commentStack[0]{
                        current_block.push_str("/*");
                    }
                    pos += 2;
                } else if pos + 1 < chars.len() && chars[pos] == '*' && chars[pos + 1] == '/' {
                    match commentStack[0]{
                        commentType::doc => {
                            let comment_pop=commentStack.pop();
                            pos += 2;
                        },
                        commentType::inline => {
                            let comment_pop=commentStack.pop();
                            current_block.push_str("*/");
                            pos += 2;
                            if commentStack.is_empty() {
                                // 块注释结束，将收集到的块注释保存
                                comments.push(current_block.trim().to_string());
                                current_block.clear();
                            }
                        },
                    }
                } else {
                    if let commentType::inline=commentStack[0]{
                        current_block.push(chars[pos]);
                    }
                    pos += 1;
                }
            }
        }
        //println!("after a line {:?}",comments);
        //println!("after a line current_block{:?}",current_block);
        // 如果本行结束后仍处于块注释中，则换行继续累积内容
        if !commentStack.is_empty() {
            current_block.push('\n');
        }
        i+=1;
    }
    
    // 如果块注释没有正确闭合，仍将当前内容保存
    if !current_block.trim().is_empty() {
        comments.push(current_block.trim().to_string());
    }
    comments

}

/// 提取指定范围内的注释，包括函数定义前的注释和函数体内的注释。
/// - extracted_start_line: 目标函数起始行号（1-indexed）
/// - extracted_end_line: 目标函数结束行号（1-indexed）
enum commentType{
    doc,
    inline,
}
fn extract_inline_comments(source: &str, extracted_start_line: usize, extracted_end_line: usize) -> Vec<String> {
    let lines: Vec<&str> = source.lines().collect();
    let mut result = Vec::new();

    //let mut res_before_comment=Vec::new();
    let mut comments = Vec::new();
    let mut commentStack = Vec::new();         // 块注释嵌套计数器
    let mut current_block = String::new(); // 当前正在收集的块注释内容
    //let mut inside_doc=0;
    let mut i=0;
    while i<extracted_start_line-1{
        // println!("before a line {:?}",comments);
        let line=lines[i];
        let chars: Vec<char> = line.chars().collect();
        let mut pos = 0;
        //println!("now line: {}",line);
        while pos < chars.len() {
            //println!("now char: {}", chars[pos]);
            //println!("now current_block: {}", current_block);
            if commentStack.is_empty() {
                // 检查是否是单行注释
                if pos + 1 < chars.len() && chars[pos] == '/' && chars[pos + 1] == '/' {
                    // 直接将本行后半部分作为单行注释
                    if (pos + 2 < chars.len()&& chars[pos+2]!='/' && chars[pos+2]!='!')
                    {
                        //println!("before push {:?}",comments);
                        let comment: String = chars[pos..].iter().collect();
                        //println!("push // {}",comment);
                        comments.push(comment.trim().to_string());
                        //println!("after push // {:?}",comments);
                        break; // 当前行处理完毕
                    }
                    else{
                        break;
                    }
                }
                // 检查是否是块注释的起始标记 "/*"
                else if pos + 1 < chars.len() && chars[pos] == '/' && chars[pos + 1] == '*' {
                    if (pos + 2 < chars.len()&&chars[pos+1]!='*'&&chars[pos+2]!='!')
                    {
                        commentStack.push(commentType::inline);
                        current_block.push_str("/*");
                        pos += 2;
                    }else{
                        commentStack.push(commentType::doc);
                        pos += 3;
                    }
                } else {
                    if (!comments.is_empty()&&chars[pos]!=' '){
                        comments.clear();
                    }
                    pos += 1;
                }
            } else {
                // 已经在块注释中，处理嵌套情况
                if pos + 1 < chars.len() && chars[pos] == '/' && chars[pos + 1] == '*' {
                    commentStack.push(commentType::inline);
                    if let commentType::inline=commentStack[0]{
                        current_block.push_str("/*");
                    }
                    pos += 2;
                } else if pos + 1 < chars.len() && chars[pos] == '*' && chars[pos + 1] == '/' {
                    match commentStack[0]{
                        commentType::doc => {
                            let comment_pop=commentStack.pop();
                            pos += 2;
                        },
                        commentType::inline => {
                            let comment_pop=commentStack.pop();
                            current_block.push_str("*/");
                            pos += 2;
                            if commentStack.is_empty() {
                                // 块注释结束，将收集到的块注释保存
                                comments.push(current_block.trim().to_string());
                                current_block.clear();
                            }
                        },
                    }
                } else {
                    if let commentType::inline=commentStack[0]{
                        current_block.push(chars[pos]);
                    }
                    pos += 1;
                }
            }
        }
        //println!("after a line {:?}",comments);
        //println!("after a line current_block{:?}",current_block);
        // 如果本行结束后仍处于块注释中，则换行继续累积内容
        if !commentStack.is_empty() {
            current_block.push('\n');
        }
        i+=1;
    }
    
    // 如果块注释没有正确闭合，仍将当前内容保存
    if !current_block.trim().is_empty() {
        comments.push(current_block.trim().to_string());
    }
    result.extend(comments);


    // 2. 提取函数体内部的注释（从 extracted_start_line 到 extracted_end_line 行）
    if extracted_start_line - 1 < lines.len() && extracted_end_line <= lines.len() {
        //println!("start extract inline:{:?}",result);
        let inside_lines: Vec<&str> = lines[extracted_start_line - 1 .. extracted_end_line].iter().cloned().collect();
        let inside_comments = extract_comments_from_lines(&inside_lines);
        //println!("after extract inline commet:{:?}",inside_comments);
        result.extend(inside_comments);
        //println!("after extract inline:{:?}",result);
    }

    result
}

enum FunctionMacroType {
    ItemFn(ItemFn),
    ForeignItemFn(ForeignItemFn),
    ImplItemMethod(ImplItemMethod),
    ItemMacro(ItemMacro),
    ItemMacro2(ItemMacro2),
    //ForeignItemMacro(ForeignItemMacro),
    //ImplItemMacro(ImplItemMacro),
}

fn find_foreign_function (item:&ForeignItem,target_line: usize)-> Option<FunctionMacroType>{
    match item{
        ForeignItem::Fn(foreign_item_fn) => {
            let start_line = foreign_item_fn.span().start().line;
            let end_line=foreign_item_fn.span().end().line;
            if start_line <= target_line && end_line >=target_line  
            {
                return Some(FunctionMacroType::ForeignItemFn(foreign_item_fn.clone()));
            }else{
                return None;
            }
        },
        //ForeignItem::Static(foreign_item_static) => todo!(),
        //ForeignItem::Type(foreign_item_type) => todo!(),
        /*ForeignItem::Macro(foreign_item_macro) => {
            let start_line = foreign_item_macro.span().start().line;
            let end_line=foreign_item_macro.span().end().line;
            if start_line <= target_line && end_line >=target_line  
            {
                return Some(FunctionMacroType::ForeignItemMacro(foreign_item_macro.clone()));
            }else{
                return None;
            }
        },*/
        //ForeignItem::Verbatim(token_stream) => todo!(),
        _ => todo!(),
    }
}

fn find_function_item(item:&Item,target_line: usize) ->Option<FunctionMacroType>{
    match item{
        //Item::Const(item_const) => {return None;},
        //Item::Enum(item_enum) => {},
        //Item::ExternCrate(item_extern_crate) => {},
        Item::Fn(item_fn) => {
            let start_line = item_fn.span().start().line;
            let end_line=item_fn.span().end().line;
            if start_line <= target_line && end_line >=target_line  
            {
                return Some(FunctionMacroType::ItemFn(item_fn.clone()));
            }else{
                return None;
            }
        },
        Item::ForeignMod(item_foreign_mod) => {
            for foreign_item in &item_foreign_mod.items{
                match foreign_item{
                    ForeignItem::Fn(foreign_item_fn) => {
                        let start_line = foreign_item_fn.span().start().line;
                        let end_line=foreign_item_fn.span().end().line;
                        if start_line <= target_line && end_line >=target_line  
                        {
                            return Some(FunctionMacroType::ForeignItemFn(foreign_item_fn.clone()));
                        }else{
                            return None;
                        }
                    },
                    //ForeignItem::Static(foreign_item_static) => todo!(),
                    //ForeignItem::Type(foreign_item_type) => todo!(),
                    /*ForeignItem::Macro(foreign_item_macro) => {
                        let start_line = foreign_item_macro.span().start().line;
                        let end_line=foreign_item_macro.span().end().line;
                        if start_line <= target_line && end_line >=target_line  
                        {
                            return Some(FunctionMacroType::ForeignItemMacro(foreign_item_macro.clone()));
                        }else{
                            return None;
                        }
                    },*/
                    ///ForeignItem::Verbatim(token_stream) => todo!(),
                    _ => todo!(),
                }
            }
            return None;
        },
        Item::Impl(item_impl) =>{
            for impl_item in &item_impl.items{
                match impl_item{
                    syn::ImplItem::Const(impl_item_const) => {
                    },
                    syn::ImplItem::Method(impl_item_method) => {
                        let start_line = impl_item_method.span().start().line;
                        let end_line=impl_item_method.span().end().line;
                        if start_line <= target_line && end_line >=target_line  
                        {
                            return Some(FunctionMacroType::ImplItemMethod(impl_item_method.clone()))
                        }
                    },
                    syn::ImplItem::Type(impl_item_type) => {},
                    syn::ImplItem::Macro(impl_item_macro) => {
                        /*let start_line = impl_item_macro.span().start().line;
                        let end_line=impl_item_macro.span().end().line;
                        if start_line <= target_line && end_line >=target_line  
                        {
                            return Some(FunctionMacroType::ImplItemMacro(impl_item_macro.clone()))
                        }*/
                    },
                    syn::ImplItem::Verbatim(token_stream) => {},
                    _ => todo!(),
                }
            }
            return None;
        },
        Item::Macro(item_macro) => {
            let start_line = item_macro.span().start().line;
            let end_line=item_macro.span().end().line;
            if start_line <= target_line && end_line >=target_line  
            {
                return Some(FunctionMacroType::ItemMacro(item_macro.clone()));
            };
            return None;
        },
        Item::Macro2(item_macro2) => {
            let start_line = item_macro2.span().start().line;
            let end_line=item_macro2.span().end().line;
            if start_line <= target_line && end_line >=target_line  
            {
                return Some(FunctionMacroType::ItemMacro2(item_macro2.clone()));
            };
            return None;
        },
        Item::Mod(item_mod) => {
            let mod_start_line=item_mod.span().start().line;
            let mod_end_line=item_mod.span().end().line;
            if mod_start_line <= target_line && mod_end_line >=target_line  
            {
                match &item_mod.content{
                    Some((_,mod_items)) => {
                        for mod_item in mod_items{
                            match find_function_item(mod_item, target_line){
                                Some(res) =>{return Some(res)},
                                None => {},
                            }
                        }
                        return None;
                    },
                    None => {return None;},
                }
            }
            else{
                return None;
            }
        },
        //Item::Static(item_static) => {},
        //Item::Struct(item_struct) => {},
        //Item::Trait(item_trait) => {},
        //Item::TraitAlias(item_trait_alias) => {},
        //Item::Type(item_type) => {},
        //Item::Union(item_union) => {},
        //Item::Use(item_use) => {},
        //Item::Verbatim(token_stream) => {},
        _ =>{return None;},
    }
}

/// 在 AST 中查找起始行号匹配的函数
fn find_function_by_start_line(ast: &File, target_line: usize) -> Option<FunctionMacroType> {
    /*  for item in items {
        match item {
            Item::Mod(module) => {
                println!("Found module: {}", module.ident);
                if let Some((_, items)) = &module.content {
                    visit_items(items);
                }
            }
            Item::Fn(function) => {
                println!("Found function: {}", function.sig.ident);
            }
            _ => {}
        }
    } */
    for item in &ast.items {
        match find_function_item(item, target_line){
            Some(res) => return Some(res),
            None => {},
        }
    }
    return None;
}

use serde::Deserialize;
use std::collections::HashMap;

#[derive(Debug, Deserialize)]
struct Root {
    // 跳过 creation_date
    #[serde(rename = "crates")]
    crates_list: Vec<CrateEntry>,
}

#[derive(Debug, Deserialize)]
struct CrateEntry {
    // JSON 里键名是 "Package"
    #[serde(rename = "Package")]
    package: Package,
}

#[derive(Debug, Deserialize)]
struct Package {
    name: String,
    version: String,
}

fn main() {
    // 程序参数:
    // args[1]:CSV 文件路径（记录中包含目标函数信息）
    // args[2]:cache root
    // args[3]:resul folder
    let args: Vec<String> = env::args().collect();
    println!("num:{}",args.len());
    if args.len() < 4 {
        eprintln!("Usage: {} <functions_csv> <crate_list_directory> <crates_cache_root> <result_directory>", args[0]);
        std::process::exit(1);
    }
    let csv_path = Path::new(&args[1]);
    //let crate_list = Path::new(&args[2]);
    let cache_root=Path::new(&args[2]);
    let result_root=Path::new(&args[3]);

    //let crate_list_data = fs::read_to_string(crate_list).expect("cannot read crate_list file");
    // 2. 反序列化到 Root
    //let crate_list_root: Root = serde_json::from_str(&crate_list_data).expect("cannot deserialize crate list");
    // 3. 遍历并收集到 HashMap
    //let mut crate_list_map: HashMap<String, Package> = HashMap::new();
    //for entry in crate_list_root.crates_list {
        // 以包名为键，整个 Package 结构体为值
        //crate_list_map.insert(entry.package.name.clone(), entry.package);
    //}

    let mut rdr = ReaderBuilder::new()
        .has_headers(false)
        .from_path(csv_path)
        .unwrap_or_else(|e| panic!("Unable to read CSV file: {}", e));

    let mut results = Vec::new();
    // 解析 CSV 记录，假定格式为：
    // - 第3列（索引2）：def_path
    // - 第9列（索引8）：文件相对路径
    // - 第10列（索引9）：函数起始行号（1-indexed）
    println!("start extract csv!");
    let mut crate_name=String::new();
    //let mut crate_found_flag=true;
    let mut crate_root=String::new();
    //let mut crate_name_path_map:HashMap<String, String> = HashMap::new();
    let mut all_extracted_function_num=0;
    for result in rdr.records() {
        let record = result.expect("Error reading CSV record");
        if record.len() < 10 {
            continue;
        }
        let new_crate_name=record.get(1).unwrap().to_string().replace('_', "-");
        let function_safety=record.get(12).unwrap();
        let item_id=record.get(0).unwrap().to_string();
        let def_path = record.get(3).unwrap().to_string();
        let rel_file = record.get(9).unwrap();
        let start_line: usize = record.get(10).unwrap().parse().unwrap_or_else(|e| {
            panic!("Failed to parse start line: {}", e)
        });
        //println!("{}",function_safety);
        println!("now function: {} {} {} {} {}", &item_id,&new_crate_name,&def_path,&rel_file,&start_line);
        if (!function_safety.eq("Safe")){
            continue;
        }
        all_extracted_function_num+=1;
        if !new_crate_name.eq(&crate_name){

            //let new_package=crate_list_map.get(&new_crate_name);
            //match new_package{
                //Some(package_content) => {
                    //crate_found_flag=true;
                    //package_name=package_content.name.clone();
                    //package_version=package_content.version.clone();
                //},
                //None => {crate_found_flag=false;},
            //}
            //let crate_file_name=package_name+"-"+package_version;

            if (!results.is_empty()){
                let output_file_name="result-".to_owned()+&crate_name.clone()+".json";
                let output_path = result_root.join(output_file_name);
                let json = serde_json::to_string_pretty(&results)
                    .expect("Failed to serialize to JSON");
            
                let mut result_file = OpenOptions::new()
                    .create(true)   // 文件不存在时创建
                    .append(true)   // 每次写入都追加到末尾，而不截断
                    .open(&output_path).expect("failed to open or create result.json");
            
                // 将 JSON 文本及换行写入文件末尾
                if let Err(e) = result_file.write_all(json.as_bytes()) {
                    eprintln!("Failed to append to {:?}: {}", output_path, e);
                    return;
                }
                if let Err(e) = result_file.write_all(b"\n") {
                    eprintln!("Failed to append newline to {:?}: {}", output_path, e);
                    return;
                }
                results.clear();

                println!("Results written of {} to {:?}", crate_name,output_path); 

                let now_crate_root_path=Path::new(&crate_root);
                if now_crate_root_path.exists() {
                    match fs::remove_dir_all(&now_crate_root_path){
                        Ok(_) => {
                            println!("has deleted {:?}", &now_crate_root_path);
                        }
                        Err(_) => {
                            println!("failed to delete {:?}", &now_crate_root_path);
                        },
                    }
                    
                } else {
                    println!("the dir does not exist {:?}", &now_crate_root_path);
                }                
            }
            crate_name=new_crate_name;
            //match crate_name_path_map.get(&crate_name){
                //Some(crate_root_path) => {crate_root=crate_root_path.clone();},
                //None =>{
                    let target_crate_path=cache_root.join(&crate_name);
                    if !target_crate_path.exists() || !target_crate_path.is_dir() {
                        println!("crate name{:?} does not exit or is not a dir", &crate_name);

                    }
                     let mut zip_path: Option<PathBuf> = None;
                    let mut target_crate_file_count=0;
                    let read_target_crate_path_res = fs::read_dir(&target_crate_path);
                    let entries = match read_target_crate_path_res {
                        Ok(rd) => rd,
                        Err(e) => {
                            println!("cannot read dir {:?}: {}", target_crate_path, e);
                            panic!("cannot read dir");
                        }
                    };  
                    // 3. 寻找 .zip 并解压
                    for entry_res in entries {
                        let entry = match entry_res {
                            Ok(en) => en,
                            Err(e) => {
                                println!("cannot read item in {:?} error: {}", target_crate_path, e);
                                continue;
                            }
                        };

                        let item_path = entry.path();
                        if item_path.extension().and_then(|e| e.to_str()).map_or(false, |ext| ext.eq_ignore_ascii_case("crate")) 
                        {
                            zip_path = Some(item_path);
                            break;
                        }
                    }
                    let zip_crate_path = match zip_path{
                        Some(p) => p,
                        None => {
                            println!("cannot find crate in {:?} ", target_crate_path);
                            panic!("cannot find any crate")
                        }
                    };
                
                    // 3. 打开 .crate（实际上是 gzipped tarball）
                    let zip_file_res = fs::File::open(&zip_crate_path);
                    let zip_file = match zip_file_res {
                        Ok(f) => f,
                        Err(e) => {
                            println!("cannot open file {:?}: {}", zip_crate_path, e);
                            panic!("cannot open file")
                        }
                    };
                
                    // 4. 解压 GzDecoder -> tar Archive
                    let decoder_res = GzDecoder::new(zip_file);
                    // GzDecoder::new 直接返回，不会失败构造，但在读取时会报错
                    let mut archive = Archive::new(decoder_res);
                
                    // 5. 提取所有条目到同一目录
                    match archive.unpack(&target_crate_path) {
                        Ok(()) => {
                            println!("success unzip {:?} to {:?}", zip_crate_path, &target_crate_path);
                        }
                        Err(e) => {
                            println!("failed to unzip {:?} : {}", zip_crate_path, e);
                        }
                    }
                    let folder_name = zip_crate_path
                        .file_stem()                          // >>> "bitflags-2.9.0":contentReference[oaicite:2]{index=2}
                        .and_then(|s| s.to_str())
                        .unwrap_or_default();
                    let extracted_file_dir = target_crate_path.join(folder_name);
                    //println!("{:?}",&extracted_file_dir);
                    crate_root=extracted_file_dir.to_str().expect("failed tp convert extracted file path to string").to_owned();
                    //crate_name_path_map.insert(crate_name.clone(), crate_root.clone());
                //}
            //}
        }
        //return 
        let file_path: PathBuf = Path::new(&crate_root).join(rel_file);
        println!("extract: {} {:?}", def_path, file_path);
        let source = fs::read_to_string(&file_path)
            .unwrap_or_else(|e| panic!("Failed to read file {:?}: {}", file_path, e));

        // 使用 syn 解析文件
        let ast: File = syn::parse_str(&source)
            .unwrap_or_else(|e| panic!("Failed to parse file {:?}: {}", file_path, e));

        // 尝试根据 CSV 提供的起始行号查找目标函数
        let mut extracted_start_line:usize=0;
        let mut extracted_end_line:usize=0;
        //println!("strat to find ItemFn");
        let (fn_name, doc_comments) = if let Some(func) = find_function_by_start_line(&ast, start_line) {
            //println!("Success find ItemFn");
            let name = 
            match &func{
                FunctionMacroType::ItemFn(item_fn) => 
                    {
                        extracted_start_line=item_fn.span().start().line;
                        extracted_end_line=item_fn.span().end().line;
                        item_fn.sig.ident.to_string()
                    },
                FunctionMacroType::ForeignItemFn(foreign_item_fn) => 
                    {
                        extracted_start_line=foreign_item_fn.span().start().line;
                        extracted_end_line=foreign_item_fn.span().end().line;
                        foreign_item_fn.sig.ident.to_string()
                    },
                FunctionMacroType::ImplItemMethod(impl_item_method) => 
                    {
                        extracted_start_line=impl_item_method.span().start().line;
                        extracted_end_line=impl_item_method.span().end().line;
                        impl_item_method.sig.ident.to_string()
                    },
                FunctionMacroType::ItemMacro(item_macro) => 
                    {
                        extracted_start_line=item_macro.span().start().line;
                        extracted_end_line=item_macro.span().end().line;
                        item_macro.ident.clone().map(|ident| ident.to_string()).unwrap_or_default()
                    },
                FunctionMacroType::ItemMacro2(item_macro2) =>{
                    extracted_start_line=item_macro2.span().start().line;
                    extracted_end_line=item_macro2.span().end().line;
                    item_macro2.ident.to_string()
                },
                //FunctionMacroType::ForeignItemMacro(foreign_item_macro) =>{
                //    extracted_start_line=foreign_item_macro.span().start().line;
                //    extracted_end_line=foreign_item_macro.span().end().line;
                //    foreign_item_macro.ident.map(|ident| ident.to_string()).unwrap_or_default()
                //},
                //FunctionMacroType::ImplItemMacro(impl_item_macro) => {
                //    extracted_start_line=impl_item_macro.span().start().line;
                //    extracted_end_line=impl_item_macro.span().end().line;
                //    impl_item_macro.ident.map(|ident| ident.to_string()).unwrap_or_default()
                //},
            };
            (name, extract_doc_comments(&func))
        } else {
            // 如果未能通过 AST 定位，则通过文本扫描尝试从指定行解析函数名
            /*let lines: Vec<&str> = source.lines().collect();
            let name = if start_line - 1 < lines.len() {
                let line = lines[start_line - 1];
                if let Some(idx) = line.find("fn ") {
                    let rest = &line[idx + 3..];
                    if let Some(end) = rest.find(|c: char| c.is_whitespace() || c == '(') {
                        rest[..end].to_string()
                    } else {
                        "unknown".to_string()
                    }
                } else {
                    "unknown".to_string()
                }
            } else {
                "unknown".to_string()
            };*/
            panic!("Failed to find_function_by_start_line {} {} {}",def_path,rel_file,start_line);
            //("Failed to find_function_by_start_line".to_string(), Vec::new())
        };

        let has_doc = !doc_comments.is_empty();
        let doc_paragraph = doc_comments.join(" ");
        println!("Success find doc comments {}",doc_paragraph);
        //println!("Success find doc comments");

        // 使用文本扫描提取普通注释（基于函数名定位）
        //println!("Start extract_inline_comments {} {}",extracted_start_line,extracted_end_line);
        let inline_comments = extract_inline_comments(&source, extracted_start_line,extracted_end_line);
        println!("Success extract_inline_comments");
        let has_inline_comment = !inline_comments.is_empty();
        let inline_comment_paragraph = inline_comments.join(" ");
        println!("Success find normal comments");

        results.push(FunctionCommentStatus {
            crate_name:crate_name.clone(),
            def_path,
            file:rel_file.to_string(),
            line:extracted_start_line,
            has_doc,
            doc_paragraph,
            has_inline_comment,
            inline_comment_paragraph,
        });
    }

    let output_file_name="result-".to_owned()+&crate_name.clone()+".json";
    let output_path = result_root.join(output_file_name);
    let json = serde_json::to_string_pretty(&results)
        .expect("Failed to serialize to JSON");

    let mut result_file = OpenOptions::new()
        .create(true)   // 文件不存在时创建
        .append(true)   // 每次写入都追加到末尾，而不截断
        .open(&output_path).expect("failed to open or create result.json");

    // 将 JSON 文本及换行写入文件末尾
    if let Err(e) = result_file.write_all(json.as_bytes()) {
        eprintln!("Failed to append to {:?}: {}", output_path, e);
        return;
    }
    if let Err(e) = result_file.write_all(b"\n") {
        eprintln!("Failed to append newline to {:?}: {}", output_path, e);
        return;
    }

    //println!("Results appended to {:?}", output_path);
    //fs::write(&output_path, json)
    //    .expect(&format!("Failed to write JSON to file: {:?}", output_path));
    println!("Results written to {:?}", output_path);
    
    let now_crate_root_path=Path::new(&crate_root);
    if now_crate_root_path.exists() {
        match fs::remove_dir_all(&now_crate_root_path){
            Ok(_) => {
                println!("has deleted {:?}", &now_crate_root_path);
            }
            Err(_) => {
                println!("failed to delete {:?}", &now_crate_root_path);
            },
        }
        
    } else {
        println!("the dir does not exist {:?}", &now_crate_root_path);
    }     

    println!("extracted function count {}", all_extracted_function_num);
}
