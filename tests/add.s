
        section .text
        global our_code_starts_here
        our_code_starts_here:
        
               mov rax, 100
               mov [rsp - 16], rax
               mov rax, 1
               add rax, [rsp - 16]
            
        ret
        