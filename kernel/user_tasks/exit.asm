section .text
        global main

main:
        mov rax, 1
        mov rdi, 0
        int 0x80
