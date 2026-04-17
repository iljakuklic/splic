(module
  (type (;0;) (func (param i64) (result i64)))
  (type (;1;) (func (param i64) (result i64)))
  (export "double" (func 0))
  (export "quadruple" (func 1))
  (func (;0;) (type 0) (param i64) (result i64)
    local.get 0
    local.get 0
    i64.add
  )
  (func (;1;) (type 1) (param i64) (result i64)
    local.get 0
    call 0
    call 0
  )
)
