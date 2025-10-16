use std::env;
use std::fs::File;
use std::io::prelude::*;
use sexp::*;
use sexp::Atom::*;
use dynasmrt::{dynasm, DynasmApi};
use std::mem;
use std::collections::HashMap;
use std::io::{self, Write};
enum Op1 {
    Add1,
    Sub1,
}


enum Op2 {
    Plus,
    Minus,
    Times
}

enum Expr {
    Number(i32),
    Id(String),
    Let(Vec<(String, Expr)>, Box<Expr>),
    UnOp(Op1, Box<Expr>),
    BinOp(Op2, Box<Expr>, Box<Expr>),
    Define(String, Box<Expr>),
}


fn parse_expr(s: &Sexp, define_cnt: &mut i32, flag: &String) -> Expr {
    match s {
        Sexp::Atom(Atom::I(n)) => Expr::Number(i32::try_from(*n).unwrap()),
        Sexp::Atom(Atom::S(n)) => Expr::Id(n.clone()),
        Sexp::List(vec) => {
            match &vec[..] {
                [Sexp::Atom(S(op)), e] if op == "add1" => Expr::UnOp(Op1::Add1, Box::new(parse_expr(e, define_cnt, flag))),
                [Sexp::Atom(S(op)), e] if op == "sub1" => Expr::UnOp(Op1::Sub1, Box::new(parse_expr(e, define_cnt, flag))),
                [Sexp::Atom(S(op)), e1, e2] if op == "+" => Expr::BinOp(Op2::Plus, Box::new(parse_expr(e1, define_cnt, flag)), 
                    Box::new(parse_expr(e2, define_cnt, flag))),
                [Sexp::Atom(S(op)), e1, e2] if op == "-" => Expr::BinOp(Op2::Minus, Box::new(parse_expr(e1, define_cnt, flag)), 
                    Box::new(parse_expr(e2, define_cnt, flag))),
                [Sexp::Atom(S(op)), e1, e2] if op == "*" => Expr::BinOp(Op2::Times, Box::new(parse_expr(e1, define_cnt, flag)), 
                    Box::new(parse_expr(e2, define_cnt, flag))),
                [Sexp::Atom(S(op)), Sexp::Atom(Atom::S(n)), e] if op == "define" => {
                    *define_cnt += 1;
                    if *define_cnt > 1 {
                        panic!("Invalid: parse error");
                    }
                    if flag != "-i" {
                        panic!("Invalid: parse error");
                    }
                    Expr::Define(n.clone(),Box::new(parse_expr(e, define_cnt, flag)))
                }
                [Sexp::Atom(S(op)), Sexp::List(bindings), e] if op == "let" => {
                    let bindings = parse_bind(bindings, define_cnt, flag);
                    Expr::Let(bindings, Box::new(parse_expr(e, define_cnt, flag)))
                }
                _ => panic!("Invalid: parse error"),
            }
        },
        _ => panic!("Invalid: parse error"),
    }
}


fn parse_bind(bindings: &Vec<Sexp>, define_cnt:&mut i32, flag:&String) -> Vec<(String, Expr)> {
    let mut seen_names = std::collections::HashSet::new();
    
    bindings.iter().map(|binding| {
        match binding {
            Sexp::List(pair) => match &pair[..] {
                [Sexp::Atom(Atom::S(var_name)), exp] => {
                    // Check for duplicate
                    if !seen_names.insert(var_name.clone()) {
                        panic!("Duplicate binding");
                    }
                    (var_name.clone(), parse_expr(exp, define_cnt, flag))
                }
                _ => panic!("Invalid binding pair: {:?}", pair),
            },
            _ => panic!("Each binding should be a list"),
        }
    }).collect()
}


fn main() -> std::io::Result<()> {
    let args: Vec<String> = env::args().collect();
    
    let flag = &args[1];
    let mut def_cnt = 0;
    match flag.as_str() {
        "-i" => {
            if args.len() != 2 {
                eprintln!("Error: -i requires no args");
                std::process::exit(1);
            }
            repl(flag);

        }
        "-c" => {
            let in_name = &args[2];

            let mut in_file = File::open(in_name)?;
            let mut in_contents = String::new();
            in_file.read_to_string(&mut in_contents)?;
            let expr = parse_expr(&parse(&in_contents).unwrap(), &mut def_cnt, flag);
            // Compile to assembly only
            if args.len() < 4 {
                eprintln!("Error: -c requires output file");
                std::process::exit(1);
            }
            let out_name = &args[3];
            let env = HashMap::new();
            let mut define_env = HashMap::new();
            let result = compile_expr(&expr, 2, &env, &define_env);
            let asm_program = format!("
        section .text
        global our_code_starts_here
        our_code_starts_here:
        {}
        ret
        ", result);
            let mut out_file = File::create(out_name)?;
            out_file.write_all(asm_program.as_bytes())?;
        }
        "-e" => {
            let mut ops = dynasmrt::x64::Assembler::new().unwrap();
            let start = ops.offset();
            let env_ops = HashMap::new();
            let mut define_env = HashMap::new();
            let in_name = &args[2];
    
            let mut in_file = File::open(in_name)?;
            let mut in_contents = String::new();
            in_file.read_to_string(&mut in_contents)?;
            let expr = parse_expr(&parse(&in_contents).unwrap(), &mut def_cnt, flag);
            compile_ops(&expr, &mut ops, 2, &env_ops, &mut define_env);
            dynasm!(ops ; .arch x64 ; ret);
            let buf = ops.finalize().unwrap();
            let jitted_fn: extern "C" fn() -> i64 = unsafe { mem::transmute(buf.ptr(start)) };
            let result = jitted_fn();
            println!("{}", result);
        }
        "-g" => {
            
            let in_name = &args[2];

            let mut in_file = File::open(in_name)?;
            let mut in_contents = String::new();
            in_file.read_to_string(&mut in_contents)?;
            let expr = parse_expr(&parse(&in_contents).unwrap(), &mut def_cnt, flag);

            if args.len() < 4 {
                eprintln!("Error: -g requires output file");
                std::process::exit(1);
            }
            let out_name = &args[3];
            
            // Write assembly
            let env = HashMap::new();
            let mut define_env = HashMap::new();
            let result = compile_expr(&expr, 2, &env, &define_env);
            let asm_program = format!("
        section .text
        global our_code_starts_here
        our_code_starts_here:
        {}
        ret
        ", result);
            let mut out_file = File::create(out_name)?;
            out_file.write_all(asm_program.as_bytes())?;
            
            let mut ops = dynasmrt::x64::Assembler::new().unwrap();
            let start = ops.offset();
            let env_ops = HashMap::new();
            let mut define_env = HashMap::new();
            compile_ops(&expr, &mut ops, 2, &env_ops, &mut define_env);
            dynasm!(ops ; .arch x64 ; ret);
            let buf = ops.finalize().unwrap();
            let jitted_fn: extern "C" fn() -> i64 = unsafe { mem::transmute(buf.ptr(start)) };
            let result = jitted_fn();
            println!("{}", result);
        }
        _ => {
            eprintln!("Unknown flag: {}. Use -c, -e, or -g", flag);
            std::process::exit(1);
        }
    }
    
    Ok(())
}


//CLAUDE USAGE: I used Claude for advice on the error handling. Prompt: Sicne there is no try catch
//in rust, how can i catch errors thrown by parse_expr and compile_ops? Other than that I did not
//use AI anywhere else
fn repl(flag: &String) -> io::Result<()> {
    let mut ops = dynasmrt::x64::Assembler::new().unwrap();
    let mut define_env = HashMap::new();

    loop {
        let mut define_count = 0;
        //write > to stdout 
        let start = ops.offset();
        let mut buffer = String::new();
        io::stdout().write_all(b">");
        io::stdout().flush()?;
        io::stdin().read_line(&mut buffer)?;

        let trimmed_string = buffer.trim_end();
        if trimmed_string.is_empty() {
           continue;
        }
        if trimmed_string == "quit" || trimmed_string == "exit" {
            break;
        }


        let parsed = match parse(trimmed_string) {
            Ok(p) => p,
            Err(e) => {
                println!("{}", e);
                continue;
            }
        };

        let expr = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            parse_expr(&parsed, &mut define_count, flag)
        })) {
            Ok(e) => e,
            Err(e) => {
                let msg = e.downcast_ref::<&str>()
                .map(|s| s.to_string())
                .or_else(|| e.downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "Unknown error".to_string());
                continue;
            }
        };

       match &expr {
         Expr::Id(var) => {
            if define_env.get(var).is_some() {
                let result_str = format!("{}\n", define_env.get(var).unwrap());
                io::stdout().write_all(result_str.as_bytes())?;
                io::stdout().flush()?;
                continue;
            }
            else {
                print!("Unbound variable");
            }
          }
          _ => {}
       }

       let env = HashMap::new();
       if let Err(e) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            compile_ops(&expr, &mut ops, 2, &env, &define_env);
       })) {
            let msg = e.downcast_ref::<&str>()
            .map(|s| s.to_string())
            .or_else(|| e.downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "Unknown error".to_string());
            continue;
       }
       dynasm!(ops ; .arch x64 ; ret);
       ops.commit().unwrap();
       let reader = ops.reader();
       let buf = reader.lock();
       let jitted_fn: extern "C" fn() -> i64 = unsafe { mem::transmute(buf.ptr(start)) };
       let result = jitted_fn();
       
       //check if define statement in 
       match expr {
         Expr::Define(var, expr) => {
            define_env.insert(var, result);
            continue;
         }
         _ => {}
       }

       //copied these three line from chat output result
       let result_str = format!("{}\n", result);
       io::stdout().write_all(result_str.as_bytes())?;
       io::stdout().flush()?; 
    }
    Ok(())
    

}

fn compile_expr(e: &Expr, si: i32, env: &HashMap<String, i32>, define_env: &HashMap<String, i64>) -> String {
    match e {
        Expr::Number(n) => format!("mov rax, {}", *n),
        Expr::Id(name) => {
            let stack_offset = env.get(name).expect(&format!("Unbound variable identifier {}", name));
            format!("mov rax, [rsp - {}]", stack_offset)
        },
        Expr::UnOp(Op1::Add1, subexpr) => compile_expr(subexpr, si, env, define_env) + "\nadd rax, 1",
        Expr::UnOp(Op1::Sub1, subexpr) => compile_expr(subexpr, si, env, define_env) + "\nsub rax, 1",
        Expr::BinOp(Op2::Plus, e1, e2) => {
            let expr1_instrs = compile_expr(e1, si, env, define_env);
            let expr2_instrs = compile_expr(e2, si+1, env, define_env);
            let stack_offset = si*8;
            format!("
               {expr1_instrs}
               mov [rsp - {stack_offset}], rax
               {expr2_instrs}
               add rax, [rsp - {stack_offset}]
            ")
        },
        Expr::BinOp(Op2::Minus, e1, e2) => {
            let expr1_instrs = compile_expr(e1, si, env, define_env);
            let expr2_instrs = compile_expr(e2, si+1, env, define_env);
            let stack_offset = si*8;
            format!("
               {expr1_instrs}
               mov [rsp - {stack_offset}], rax
               {expr2_instrs}
               sub rax, [rsp - {stack_offset}]
            ")
        },
        Expr::BinOp(Op2::Times, e1, e2) => {
            let expr1_instrs = compile_expr(e1, si, env, define_env);
            let expr2_instrs = compile_expr(e2, si+1, env, define_env);
            let stack_offset = si*8;
            format!("
               {expr1_instrs}
               mov [rsp - {stack_offset}], rax
               {expr2_instrs}
               imul rax, [rsp - {stack_offset}]
            ")
        },
        Expr::Let(bindings_vec, body) => {
            let mut new_env = env.clone();
            let mut instrs = String::new();
            let mut current_si = si;

            for (var_name, binding_expr) in bindings_vec {
                let binding_instrs = compile_expr(binding_expr, current_si, &new_env, define_env);
                let stack_offset = current_si * 8;
                instrs.push_str(&format!("
                    {binding_instrs}
                    mov [rsp - {stack_offset}], rax
                "));
                new_env.insert(var_name.clone(), stack_offset);
                current_si += 1;
            }

            let body_instrs = compile_expr(body, current_si, &new_env, define_env);
            instrs.push_str(&body_instrs);
            instrs
        }
        Expr::Define(var, e) => {
            compile_expr(e, si, &env, define_env)
        }
    }
}


fn compile_ops(e: &Expr, ops: &mut dynasmrt::x64::Assembler, si: i32, env: &HashMap<String, i32>, define_env: &HashMap<String, i64>) {
    match e {
        Expr::Number(n) => {
            dynasm!(ops ; .arch x64 ; mov rax, *n);
        }
        Expr::Id(name) => {
            let stack_offset = env.get(name).expect(&format!("Unbound variable identifier {}", name));
            dynasm!(ops ; .arch x64 ; mov rax, [rsp - *stack_offset]);
        }
        Expr::UnOp(Op1::Add1, e1) => {
            compile_ops(&e1, ops, si, env, define_env);
            dynasm!(ops ; .arch x64 ; add rax, 1);
        }
        Expr::UnOp(Op1::Sub1, e1) => {
            compile_ops(&e1, ops, si, env, define_env);
            dynasm!(ops ; .arch x64 ; sub rax, 1);
        }
        Expr::BinOp(Op2::Plus, e1, e2) => {
            compile_ops(&e1, ops, si, env, define_env);
            let stack_offset = si * 8;
            dynasm!(ops ; .arch x64 ; mov [rsp - stack_offset], rax);
            compile_ops(&e2, ops, si+1, env, define_env);
            dynasm!(ops ; .arch x64 ; add rax, [rsp - stack_offset]);
        }
        Expr::BinOp(Op2::Times, e1, e2) => {
            compile_ops(&e1, ops, si, env, define_env);
            let stack_offset = si * 8;
            dynasm!(ops ; .arch x64 ; mov [rsp - stack_offset], rax);
            compile_ops(&e2, ops, si+1, env, define_env);
            dynasm!(ops ; .arch x64 ; imul rax, [rsp - stack_offset]);
        }
        Expr::BinOp(Op2::Minus, e1, e2) => {
            compile_ops(&e1, ops, si, env, define_env);
            let stack_offset = si * 8;
            dynasm!(ops ; .arch x64 ; mov [rsp - stack_offset], rax);
            compile_ops(&e2, ops, si+1, env, define_env);
            dynasm!(ops ; .arch x64 
            ; mov [rsp-(si+1)*8], rax              
            ; mov rax, [rsp - stack_offset]  
            ; sub rax, [rsp-(si+1)*8]  
    );
}
        Expr::Let(bindings_vec, body) => {
            let mut new_env = env.clone();
            let mut current_si = si;

            // Compile each binding
            for (var_name, binding_expr) in bindings_vec {
                compile_ops(binding_expr, ops, current_si, &new_env, &define_env);
                let stack_offset = current_si * 8;
                dynasm!(ops ; .arch x64 ; mov [rsp - stack_offset], rax);
                new_env.insert(var_name.clone(), stack_offset);
                current_si += 1;
            }

            compile_ops(body, ops, current_si, &new_env, &define_env);
        }
        Expr::Define(var, expr) => {
            //let var_check = define_env.get(var).expect(&format!("Duplicate binding"));
            if define_env.get(var).is_some() {
                panic!("Duplicate binding");
            }
            compile_ops(expr, ops, si, &env, &define_env);
        }
        
    }
}
