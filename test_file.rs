fn main() {
    let result = calculate_sum(5, 10);
    println!("The result is: {}", result);
}

fn calculate_sum(a: i32, b: i32) -> i32 {
    let sum = a + b;
    println!("Adding {} and {}", a, b);
    sum
}

struct Calculator {
    value: i32,
    history: Vec<String>,
}

impl Calculator {
    fn new() -> Self {
        Calculator {
            value: 0,
            history: Vec::new(),
        }
    }

    fn add(&mut self, number: i32) -> i32 {
        self.value += number;
        self.history.push(format!("Added {}", number));
        self.value
    }

    fn get_result(&self) -> i32 {
        println!("Current result: {}", self.value);
        self.value
    }
}