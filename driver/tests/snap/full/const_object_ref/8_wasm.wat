(module
  (type (;0;) (func (result i32)))
  (type (;1;) (func (result i32)))
  (export "answer" (func 0))
  (export "double_answer" (func 1))
  (func (;0;) (type 0) (result i32)
    i32.const 42
  )
  (func (;1;) (type 1) (result i32)
    call 0
    call 0
    i32.add
  )
)
