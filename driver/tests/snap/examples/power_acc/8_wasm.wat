(module
  (type (;0;) (func (param i64) (result i64)))
  (type (;1;) (func (param i64) (result i64)))
  (type (;2;) (func (param i64) (result i64)))
  (type (;3;) (func (param i64) (result i64)))
  (type (;4;) (func (param i64) (result i64)))
  (type (;5;) (func (param i64) (result i64)))
  (export "pow1" (func 0))
  (export "pow5" (func 1))
  (export "pow11_inc" (func 2))
  (export "pow15" (func 3))
  (export "pow16" (func 4))
  (export "pow17" (func 5))
  (func (;0;) (type 0) (param i64) (result i64)
    (local i64)
    local.get 0
    local.set 1
    local.get 1
  )
  (func (;1;) (type 1) (param i64) (result i64)
    (local i64 i64 i64)
    local.get 0
    local.set 1
    local.get 1
    local.get 1
    i64.mul
    local.set 2
    local.get 2
    local.get 2
    i64.mul
    local.set 3
    local.get 1
    local.get 3
    i64.mul
  )
  (func (;2;) (type 2) (param i64) (result i64)
    (local i64 i64 i64 i64 i64)
    local.get 0
    i64.const 1
    i64.add
    local.set 1
    local.get 1
    local.set 2
    local.get 2
    local.get 2
    i64.mul
    local.set 3
    local.get 3
    local.get 3
    i64.mul
    local.set 4
    local.get 4
    local.get 4
    i64.mul
    local.set 5
    local.get 2
    local.get 3
    i64.mul
    local.get 5
    i64.mul
  )
  (func (;3;) (type 3) (param i64) (result i64)
    (local i64 i64 i64 i64)
    local.get 0
    local.set 1
    local.get 1
    local.get 1
    i64.mul
    local.set 2
    local.get 2
    local.get 2
    i64.mul
    local.set 3
    local.get 3
    local.get 3
    i64.mul
    local.set 4
    local.get 1
    local.get 2
    i64.mul
    local.get 3
    i64.mul
    local.get 4
    i64.mul
  )
  (func (;4;) (type 4) (param i64) (result i64)
    (local i64 i64 i64 i64 i64)
    local.get 0
    local.set 1
    local.get 1
    local.get 1
    i64.mul
    local.set 2
    local.get 2
    local.get 2
    i64.mul
    local.set 3
    local.get 3
    local.get 3
    i64.mul
    local.set 4
    local.get 4
    local.get 4
    i64.mul
    local.set 5
    local.get 5
  )
  (func (;5;) (type 5) (param i64) (result i64)
    (local i64 i64 i64 i64 i64)
    local.get 0
    local.set 1
    local.get 1
    local.get 1
    i64.mul
    local.set 2
    local.get 2
    local.get 2
    i64.mul
    local.set 3
    local.get 3
    local.get 3
    i64.mul
    local.set 4
    local.get 4
    local.get 4
    i64.mul
    local.set 5
    local.get 1
    local.get 5
    i64.mul
  )
)
