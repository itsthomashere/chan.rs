# About this project

This project will be mainly used for my other project that involved interacting with an lsp

## Goals
- Fully capable of interacting with a language server process
- Simple, unaffected by changes made the Language Server Protocol
- Memory efficient, able to attach, share multiple connection with only one Language server

## TODOS
- [ ] Handling request and return response of that very request without blocking other execution
- [ ] Handling the server's notification via continuous channels and listener
- [ ] Using the cloud or virtual file system for language server parsing
- [ ] Transform the response of the lsp to a light weight, differentiable, that will be used to transform response into html like completions items, or just normal values.
