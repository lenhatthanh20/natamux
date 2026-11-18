// Edge cases of C++ colours.
#include <string>
#include <vector>

namespace shop::ledger {

template <typename T>
concept Number = std::is_arithmetic_v<T>;

template <Number T, std::size_t N = 4>
class Ring final {
public:
    explicit Ring(T fill = T{}) noexcept : fill_{fill} {}
    T operator[](std::size_t i) const { return items_.at(i % N); }
    [[nodiscard]] constexpr std::size_t size() const { return N; }
    virtual ~Ring() = default;

private:
    T fill_;
    std::vector<T> items_ = std::vector<T>(N, fill_);
};

auto describe(const std::string& name) -> std::string {
    auto raw = R"(raw "text")";
    auto add = [&name](int a, int b) mutable { return a + b + static_cast<int>(name.size()); };
    return name + std::to_string(add(1, 2)) + raw + u8"é";
}

}  // namespace shop::ledger
