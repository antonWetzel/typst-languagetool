#let lt(overwrite: false) = {
	if not sys.inputs.at("spellcheck", default: overwrite) {
		return (doc) => doc
	}
	return (doc) => {
		show math.equation.where(block: false): it => [0]
		show math.equation.where(block: true): it => []
		show bibliography: it => []
		show par: set par(justify: false, leading: 0.65em)
		set page(height: auto)
		show block: it => it.body
		show page: set page(numbering: none)
		show heading: it => if it.level <= 3 {
			pagebreak() + it
		} else {
			it
		}
		doc
	}
}
