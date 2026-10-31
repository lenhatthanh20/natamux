// Edge cases of go.mod colours.
module example.com/shop/v2

go 1.23

toolchain go1.23.2

require (
	golang.org/x/text v0.14.0
	example.com/old v1.2.3 // indirect
)

replace example.com/old => ../old

exclude example.com/bad v0.1.0

retract [v2.0.0, v2.0.5] // broken builds
