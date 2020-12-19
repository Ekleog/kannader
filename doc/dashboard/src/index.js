import React from 'react';
import ReactDOM from 'react-dom';
import startupConfig from './config.json';

function matchesFilterElement(i, elt) {
    if (elt === '') {
        return true;
    }
    if (elt[0] === '-') {
        return !matchesFilterElement(i, elt.slice(1));
    }
    const colonPos = elt.indexOf(':');
    if (colonPos === -1) {
        return false;
    } else {
        const type = elt.slice(0, colonPos);
        const value = elt.slice(colonPos + 1);
        if (type === 'label') {
            return i.labels.some(l => l.name === value);
        } else if (type === 'milestone') {
            return i.milestone != null && i.milestone.title === value;
        } else {
            return false;
        }
    }
}

function matchesFilter(i, filter) {
    return filter.split(' ').every(elt => matchesFilterElement(i, elt));
}

function perceivedBrightness(hex) {
    const r = parseInt(hex.slice(0, 2), 16);
    const g = parseInt(hex.slice(2, 4), 16);
    const b = parseInt(hex.slice(4, 6), 16);
    return Math.sqrt(r * r * 0.299 + g * g * 0.587 + b * b * 0.114);
}

class Issue extends React.Component {
    render() {
        const badges = this.props.data.labels.map(l => {
            const fgcolor = perceivedBrightness(l.color) > 128 ? '#000000' : '#ffffff';
            const bgcolor = '#' + l.color;
            return (
                <span key={l.id}
                      className="badge badge-pill mx-1"
                      style={{color: fgcolor, backgroundColor: bgcolor}}>
                    {l.name}
                </span>
            );
        });
        return (
            <li className="list-group-item">
                <div className="d-flex">
                    <div className="flex-grow-1">
                        <a className="text-body" href={this.props.data.html_url}>
                            <strong>{this.props.data.title}</strong>
                        </a><br />
                        <small>
                            #{this.props.data.number} opened at {this.props.data.created_at} by {this.props.data.user.login}
                        </small>
                    </div>
                    <div>
                        {badges}
                    </div>
                </div>
            </li>
        );
    }
}

class List extends React.Component {
    render() {
        const filtered_issues = this.props.issues.filter(i => {
            return matchesFilter(i, this.props.search);
        });

        const order_filters = this.props.order.slice();
        order_filters.push("");

        const ordered_issues = [];
        order_filters.forEach(filter => {
            filtered_issues.forEach(i => {
                if (matchesFilter(i, filter) && !ordered_issues.includes(i)) {
                    ordered_issues.push(i);
                }
            });
        });

        const issues = ordered_issues.map(i => {
            return (
                <Issue key={i.id} data={i} />
            );
        });

        return (
            <div className="col">
                <ul className="list-group">
                    <li className="list-group-item">
                        <form>
                            <div className="input-group">
                                <div className="input-group-prepend">
                                    <div className="input-group-text">Search</div>
                                </div>
                                <input type="text"
                                       className="form-control"
                                       value={this.props.search}
                                       onChange={(s) => this.props.onSearchChange(s.target.value)} />
                            </div>
                        </form>
                    </li>
                    {issues}
                </ul>
            </div>
        );
    }
}

class Block extends React.Component {
    render() {
        const lists = this.props.lists.map((l, i) => {
            return (
                <List key={i}
                      search={l.search}
                      order={l.order || []}
                      onSearchChange={(s) => this.props.onSearchChange(i, s)}
                      issues={this.props.issues} />
            );
        });
        return (
            <div className="mt-3">
                <h1>
                    {this.props.name}
                </h1>
                <div className="row">
                    {lists}
                </div>
            </div>
        );
    }
}

class Tab extends React.Component {
    render() {
        const filtered_issues = this.props.issues.filter(i => {
            return matchesFilter(i, this.props.search);
        });
        const blocks = this.props.blocks.map((b, i) => {
            return (
                <Block key={i}
                       name={b.name}
                       lists={b.lists}
                       onSearchChange={(l, s) => this.props.onListSearchChange(i, l, s)}
                       issues={filtered_issues} />
            );
        });
        return (
            <div className="container-fluid">
                <form>
                    <div className="input-group">
                        <div className="input-group-prepend">
                            <div className="input-group-text">Search</div>
                        </div>
                        <input type="text"
                               className="form-control"
                               value={this.props.search}
                               onChange={(s) => this.props.onTabSearchChange(s.target.value)} />
                    </div>
                </form>
                {blocks}
            </div>
        );
    }
}

class Dashboard extends React.Component {
    constructor(props) {
        super(props);
        this.state = {
            currentTab: startupConfig.defaultTab,
            config: startupConfig,
            issues: []
        };
    }

    componentDidMount() {
        this.refresh()
    }

    setTab(tab) {
        this.setState({
            currentTab: tab,
            config: this.state.config,
            issues: this.state.issues
        });
    }

    changeListSearch(tab, block, list, search) {
        const config = Object.assign({}, this.state.config);
        config.tabs = Object.assign({}, config.tabs);
        config.tabs[tab] = config.tabs[tab].slice();
        config.tabs[tab].blocks[block] = Object.assign({}, config.tabs[tab].blocks[block]);
        config.tabs[tab].blocks[block].lists = config.tabs[tab].blocks[block].lists.slice();
        config.tabs[tab].blocks[block].lists[list] = Object.assign({}, config.tabs[tab].blocks[block].lists[list]);
        config.tabs[tab].blocks[block].lists[list].search = search;
        this.setState({
            currentTab: this.state.currentTab,
            config: config,
            issues: this.state.issues
        });
    }

    changeTabSearch(tab, search) {
        const config = Object.assign({}, this.state.config);
        config.tabs = Object.assign({}, config.tabs);
        config.tabs[tab].search = search;
        this.setState({
            currentTab: this.state.currentTab,
            config: config,
            issues: this.state.issues
        });
    }

    refresh() {
        const fetchFrom = (acc, url) => {
            console.log("Fetching " + url);
            fetch(url).then((resp) => {
                const links = resp.headers.get('link');
                resp.json().then(resp => {
                    const sum = acc.concat(resp)
                    this.setState({
                        currentTab: this.state.currentTab,
                        config: this.state.config,
                        issues: sum
                    });
                    if (links !== null) {
                        const nextRe = /<([^>]*)>; rel="next"/;
                        const nextLink = links.split(',').find(l => l.match(nextRe) !== null);
                        if (nextLink !== undefined) {
                            const next = nextRe.exec(nextLink)[1]
                            fetchFrom(sum, next)
                        }
                    }
                });
            });
        };
        fetchFrom([], 'https://api.github.com/repos/' + this.state.config.repo + '/issues?sort=updated')
    }

    reset() {
        this.setState({
            currentTab: this.state.currentTab,
            config: startupConfig,
            issues: this.state.issues
        })
    }
    
    render() {
        const tabs = Object.keys(this.state.config.tabs).map(t => {
            const classes = t === this.state.currentTab ? "nav-item active" : "nav-item";
            return (
                <li key={t} className={classes}>
                    <a className="nav-link" onClick={() => this.setTab(t)}>
                        {t}
                    </a>
                </li>
            );
        });
        return (
            <div className="container-fluid">
                <nav className="navbar navbar-expand-md navbar-dark bg-dark">
                    <ul className="navbar-nav mr-auto">
                        {tabs}
                    </ul>
                    <button className="btn btn-outline-danger mr-3" onClick={() => this.reset()}>
                        Reset
                    </button>
                    <button className="btn btn-outline-warning" onClick={() => this.refresh()}>
                        Refresh
                    </button>
                </nav>
                <div className="mt-2">
                    <Tab blocks={this.state.config.tabs[this.state.currentTab].blocks}
                         search={this.state.config.tabs[this.state.currentTab].search}
                         onListSearchChange={(b, l, s) => this.changeListSearch(this.state.currentTab, b, l, s)}
                         onTabSearchChange={(s) => this.changeTabSearch(this.state.currentTab, s)}
                         issues={this.state.issues} />
                </div>
            </div>
        );
    }
}


ReactDOM.render(
    <Dashboard />,
    document.getElementById('root')
);
