//! One catalog supplies menu actions and their build-time math illustrations.

pub struct Tool {
    pub name: &'static str,
    pub command: &'static str,
    pub latex: &'static str,
}

const fn tool(name: &'static str, command: &'static str, latex: &'static str) -> Tool {
    Tool {
        name,
        command,
        latex,
    }
}

pub const FUNCTIONS: &[Tool] = &[
    tool("带底数对数", "function:log_base", r"\log_{a}"),
    tool("对数", "function:log", r"\log"),
    tool("自然对数", "function:ln", r"\ln"),
    tool("常用对数", "function:lg", r"\lg"),
    tool("正弦", "function:sin", r"\sin"),
    tool("余弦", "function:cos", r"\cos"),
    tool("正切", "function:tan", r"\tan"),
    tool("余切", "function:cot", r"\cot"),
    tool("正割", "function:sec", r"\sec"),
    tool("余割", "function:csc", r"\csc"),
    tool("反正弦", "function:arcsin", r"\arcsin"),
    tool("反余弦", "function:arccos", r"\arccos"),
    tool("反正切", "function:arctan", r"\arctan"),
    tool("双曲正弦", "function:sinh", r"\sinh"),
    tool("双曲余弦", "function:cosh", r"\cosh"),
    tool("双曲正切", "function:tanh", r"\tanh"),
    tool("绝对值", "function:abs", r"\left|x\right|"),
    tool("范数", "function:norm", r"\left\lVert x\right\rVert"),
    tool("指数函数", "function:exp", r"\exp"),
    tool("向量", "function:vec", r"\vec{v}"),
    tool("上方一点", "function:dot", r"\dot{x}"),
    tool("上方两点", "function:ddot", r"\ddot{x}"),
    tool("上方横线", "function:bar", r"\bar{x}"),
    tool("上方帽号", "function:hat", r"\hat{x}"),
];

pub const SYMBOLS: &[Tool] = &[
    tool("圆周率", "symbol:pi", r"\pi"),
    tool("theta", "symbol:theta", r"\theta"),
    tool("alpha", "symbol:alpha", r"\alpha"),
    tool("beta", "symbol:beta", r"\beta"),
    tool("gamma", "symbol:gamma", r"\gamma"),
    tool("delta", "symbol:delta", r"\delta"),
    tool("epsilon", "symbol:varepsilon", r"\varepsilon"),
    tool("lambda", "symbol:lambda", r"\lambda"),
    tool("mu", "symbol:mu", r"\mu"),
    tool("rho", "symbol:rho", r"\rho"),
    tool("sigma", "symbol:sigma", r"\sigma"),
    tool("phi", "symbol:phi", r"\phi"),
    tool("omega", "symbol:omega", r"\omega"),
    tool("无穷", "symbol:infty", r"\infty"),
    tool("实数集", "symbol:reals", r"\mathbb{R}"),
    tool("整数集", "symbol:integers", r"\mathbb{Z}"),
    tool("正负号", "symbol:pm", r"\pm"),
    tool("乘号", "symbol:times", r"\times"),
    tool("除号", "symbol:div", r"\div"),
    tool("小于或等于", "symbol:leq", r"\leq"),
    tool("大于或等于", "symbol:geq", r"\geq"),
    tool("不等于", "symbol:neq", r"\neq"),
    tool("约等于", "symbol:approx", r"\approx"),
    tool("趋向", "symbol:to", r"\to"),
    tool("属于", "symbol:in", r"\in"),
    tool("不属于", "symbol:notin", r"\notin"),
    tool("子集", "symbol:subseteq", r"\subseteq"),
    tool("并集", "symbol:cup", r"\cup"),
    tool("交集", "symbol:cap", r"\cap"),
    tool("空集", "symbol:emptyset", r"\emptyset"),
    tool("偏导符号", "symbol:partial", r"\partial"),
    tool("梯度符号", "symbol:nabla", r"\nabla"),
    tool("导数标记", "symbol:prime", r"\prime"),
    tool("角度", "symbol:degree", r"{}^{\circ}"),
    tool("自然数集", "symbol:naturals", r"\mathbb{N}"),
    tool("复数集", "symbol:complexes", r"\mathbb{C}"),
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math_editor::Field;

    #[test]
    fn every_menu_item_has_a_valid_picture_and_editable_action() {
        for item in FUNCTIONS.iter().chain(SYMBOLS) {
            crate::formula::validate(item.latex).unwrap();
            let mut field = Field::new("", "");
            field.insert("math").unwrap();
            field
                .insert(item.command)
                .unwrap_or_else(|error| panic!("{}: {error}", item.name));
            field.root.validate().unwrap();
        }
    }
}
