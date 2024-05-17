#let lt(overwrite: false, seperate-sections-level: 3) = {
	if not sys.inputs.at("spellcheck", default: overwrite) {
		return (doc) => doc
	}
	return (doc) => {
		show math.equation.where(block: false): it => [0]
		show math.equation.where(block: true): it => []
		show bibliography: it => []
		set par(justify: false, leading: 0.65em, hanging-indent: 0pt)
		set page(height: auto)
		show block: it => it.body
		show page: set page(numbering: none)
		show heading: it => if it.level <= seperate-sections-level {
			pagebreak() + it
		} else {
			it
		}
		doc
	}
}
