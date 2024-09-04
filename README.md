# About this project

This project will be mainly used for my other project that involved interacting with an lsp

## Goals
- Fully capable of interacting with a language server process
- Simple, unaffected by changes made the Language Server Protocol
- Memory efficient, able to attach, share multiple connection with only one Language server(Well the Language Server Protocol IS NOT supposed to work like a web server, you suppose to only have one client for one process, but i'll figure it out)

## TODOS
- [x] Handling request and return response of that very request without blocking other execution
- [x] Handling the server's notification via continuous channels and listener
- [ ] Using the cloud or virtual file system for language server parsing (Virtual FS stil being developed by me)
- [ ] Transform the response of the lsp to a light weight, differentiable, that will be used to transform response into html like completions items, or just normal values.
- [ ] Implementing Client logics that can be send across thread boundaries

## Project's state
- Test Coverages: 0% (I'm lazy bro)
