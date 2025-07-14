def hello_world():
    print("Hello, world!")
    return "greeting"

class Calculator:
    def __init__(self):
        self.value = 0
        self.history = []

    def add(self, number):
        """Add a number to the current value"""
        self.value += number
        self.history.append(f"Added {number}")
        return self.value

    def multiply(self, factor):
        """Multiply current value by factor"""
        old_value = self.value
        self.value *= factor
        self.history.append(f"Multiplied by {factor}")
        return self.value

def main():
    calc = Calculator()
    result = calc.add(5)
    print(f"Result: {result}")
    
    result = calc.multiply(3)
    print(f"Final result: {result}")

if __name__ == "__main__":
    main()