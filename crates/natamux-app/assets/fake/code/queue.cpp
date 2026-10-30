// A queue of jobs with a priority, for the checkout workers.
#include <iostream>
#include <queue>
#include <string>
#include <vector>

namespace shop {

enum class Priority { Low = 0, Normal = 1, High = 2 };

struct Job {
    std::string name;
    Priority priority = Priority::Normal;
    bool operator<(const Job& other) const { return priority < other.priority; }
};

template <typename T>
class Queue {
public:
    void push(T item) { items_.push(std::move(item)); }
    [[nodiscard]] bool empty() const noexcept { return items_.empty(); }
    T pop() {
        T top = items_.top();
        items_.pop();
        return top;
    }

private:
    std::priority_queue<T> items_;
};

}  // namespace shop

int main() {
    shop::Queue<shop::Job> queue;
    queue.push({"send receipt", shop::Priority::Low});
    queue.push({"charge card", shop::Priority::High});
    auto names = std::vector<std::string>{};
    while (!queue.empty()) names.push_back(queue.pop().name);
    for (const auto& name : names) std::cout << name << '\n';
    return static_cast<int>(names.size()) - 2;
}
