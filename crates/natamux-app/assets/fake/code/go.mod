module example.com/shop

go 1.23

toolchain go1.23.4

require (
	github.com/google/uuid v1.6.0
	golang.org/x/sync v0.10.0
)

require golang.org/x/text v0.21.0 // indirect

replace example.com/shop/money => ./money

exclude golang.org/x/net v0.20.0
